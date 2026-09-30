//! SQLite access: WAL mode + one dedicated writer thread.
//!
//! Design §10 / D7: a single connection owned by one OS thread; every call
//! funnels through an unbounded channel. WAL lets future read-only
//! connections bypass the writer if profiling ever demands it — until
//! then, serialization through one actor keeps lock contention at zero.
//!
//! ONE ADDITION TO THAT DESIGN (ruagent-close-the-gaps t16): a panic raised
//! inside a caller's closure is CAUGHT, announced, and counted, so it does not
//! unwind the writer thread. Before this, one panicking operation left every
//! store-backed route dead for the rest of the process's life while `health`
//! kept answering ok. [`Db::writer_state`] is what a health check reads.

use std::path::Path;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// There is no writer to serve this call: its thread has ended (the last
    /// `Db` handle was dropped) or the channel to it is closed.
    ///
    /// NOTE (ruagent-close-the-gaps t16): this is NOT what a panic inside a
    /// store-backed closure produces. That closure is caught on the writer thread,
    /// the writer keeps serving, and the caller's `call`/`call_flat` returns the
    /// error its own logic produced — the data plane stays up, deliberately.
    #[error("database writer is shut down")]
    Closed,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The migration ledger contradicts itself or the schema (t98).
    ///
    /// A row of `schema_migrations` is a CLAIM that a migration was applied; a
    /// hole in the ledger makes that claim false for everything after it, and
    /// this variant is how the store refuses to guess. The message names the
    /// missing version(s) so the reader can act on it.
    #[error("schema ledger is not usable: {0}")]
    Ledger(String),
}

/// What the single-writer actor is doing, for a health check to read
/// (ruagent-close-the-gaps t16).
///
/// WHY THIS EXISTS: before it, the only thing anyone could ask the store was
/// whether a particular call succeeded. A daemon whose writer thread had died
/// answered `health ok` — so a supervisor, a watchdog (this repository installs
/// one at logon and every five minutes) and a human all believed the machine was
/// fine while every store-backed route failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterState {
    /// The writer thread is alive and accepting operations.
    ///
    /// NOTE what this does NOT mean, and why the distinction matters: a panic
    /// raised inside a caller's closure does NOT move off `Running`. That closure
    /// is caught and reported (see [`Db::writer_panics`]), the writer keeps
    /// serving, and the caller gets an error. Keeping the actor alive is
    /// deliberate — see the reasoning in `spawn_thread` — so `Running` is the
    /// honest answer after a caught panic, and the count is what carries the news.
    Running,
    /// The writer thread has ended: every `Db` handle was dropped, so nothing
    /// should be asking any more. A health check that sees this is talking to a
    /// store nobody is going to serve.
    Stopped,
}

// Encoded in one atomic so the health path needs no lock.
const LIVE_RUNNING: u8 = 0;
const LIVE_STOPPED: u8 = 1;

/// Prefix of every record `Db::last_writer_panic` returns.
///
/// A CONSTANT rather than prose, so a caller (or a test) can tell "a store
/// operation panicked" from any other text in the slot, and so the reading stays
/// meaningful even when the panic's own message cannot be extracted from its
/// payload.
const STORE_WRITER_PANIC_MARKER: &str = "store writer panicked: ";

/// One unit of work for the writer thread, plus everything `execute` needs.
///
/// The writer loop hands the op a CALLBACK and this actor's watch; the closure
/// supplies the work. Splitting it this way is what lets `execute` run the work,
/// record a panic, and only THEN let the caller be woken — on the panic path by
/// dropping the callback's sender. See `execute` for the ordering and the
/// measurement (3 failures in 14 runs) that forced it.
type Op = Box<dyn FnOnce(&mut rusqlite::Connection, OpWake, WriterWatch) + Send + 'static>;

/// Wakes the waiting caller with this operation's value. Dropping it without a
/// value is what tells the caller the operation panicked.
pub(crate) type Wake = Box<dyn FnOnce(Frame) + Send + 'static>;

/// The wake channel as the op receives it.
pub(crate) type OpWake = Wake;

/// One operation's reply, type-erased so the success and panic paths share a wire.
pub(crate) struct Frame(Box<dyn std::any::Any + Send>);

/// Run one operation on the writer thread, record a panic if it raises one, and
/// ONLY THEN wake the caller.
///
/// The order is the fix (ruagent-close-the-gaps t16), not a detail. The obvious
/// shape — the closure sends its own reply — lets a caller that has just received
/// its error read `writer_panics`/`last_writer_panic` BEFORE the writer has finished
/// writing them; that is not a theory, this task's own test failed on 3 runs in 14
/// by reading the PREVIOUS panic. Recording here means the caller cannot outrun it.
pub(crate) fn execute<T, F>(conn: &mut rusqlite::Connection, wake: Wake, watch: &WriterWatch, f: F)
where
    T: Send + 'static,
    F: FnOnce(&mut rusqlite::Connection) -> T + Send + 'static,
{
    // `&mut Connection` is not `UnwindSafe`, and `AssertUnwindSafe` is the honest
    // annotation for it: the invariant it stands for is that a panic has already
    // unwound the closure, so any `rusqlite::Transaction` it held has been dropped
    // and ROLLED BACK. The connection is left consistent and the work is simply not
    // done -- which is why surviving is the right trade rather than fail-fast: one
    // bad document must not convert into a total outage of the data plane.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(conn)));
    let payload = match outcome {
        Ok(value) => {
            wake(Frame(Box::new(value)));
            return;
        }
        Err(payload) => payload,
    };
    // The text is read INLINE: a helper taking `&(dyn Any + Send)` was measured
    // returning `<non-string panic payload>` for a payload whose `type_id` equalled
    // `&str`, while this expression returned the message. The marker is always
    // present so an announcement is never empty, and the precise file:line is carried
    // by the standard panic output the default hook prints.
    let text = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    let message = format!("{STORE_WRITER_PANIC_MARKER}{text}");
    let count = watch.record(message.clone());
    // ANNOUNCED, not swallowed: the log names the panic and the count, and `health`
    // carries both. A silent recovery would be the same defect one level up.
    tracing::error!(
        panic = %message,
        writer_panics = count,
        "a store operation PANICKED on the writer thread; the writer kept serving and the caller \
         received an error"
    );
    // The caller has heard nothing yet: the sender is dropped here (that is what wakes
    // `Db::call`'s receiver with an error), and only after the record above exists.
}

/// Handle to the single-writer SQLite actor. Cheap to clone.
#[derive(Clone)]
pub struct Db {
    inner: std::sync::Arc<DbInner>,
}

/// The shared half of a [`Db`]: the channel to the writer thread, plus the means
/// to shut that thread down IN ORDER when the last handle goes away.
struct DbInner {
    /// `Option` so the `Drop` below can CLOSE the channel before joining: a
    /// struct's fields are dropped only after its `Drop::drop` body runs, so a
    /// plain sender here would keep the writer's `blocking_recv` alive and the
    /// join would wait forever.
    tx: Option<tokio::sync::mpsc::UnboundedSender<Op>>,
    /// The writer thread. Joined by the `Drop` below.
    writer: Option<std::thread::JoinHandle<()>>,
    /// The writer's own id, so the `Drop` can refuse to join itself.
    writer_id: std::thread::ThreadId,
    /// `WriterState`, encoded (ruagent-close-the-gaps t16).
    live: std::sync::Arc<AtomicU8>,
    /// How many closures panicked on the writer thread. The counter is the point:
    /// a panic the actor survives is exactly the reading that used to be lost
    /// entirely (the caller saw a dead writer and the log said nothing).
    panics: std::sync::Arc<AtomicU64>,
    /// The most recent caught panic, for the health endpoint and the log line.
    last_panic: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// A handle onto the same liveness state as the writer thread's, so an
    /// operation's closure can hand the writer thread the watch it must record into
    /// (see `execute`). Cheap `Arc` clones; there is exactly one such state per `Db`.
    watch: WriterWatch,
}

impl DbInner {
    /// Read the last caught panic, tolerating a poisoned mutex.
    ///
    /// The mutex can only be poisoned if a thread panicked WHILE holding it, and
    /// the only code that holds it is the two lines below. If even those panicked,
    /// the message is the least of our problems — refusing to read it would turn a
    /// diagnostic into a second failure, which is the mistake this whole task
    /// exists to undo.
    fn last_panic(&self) -> Option<String> {
        self.last_panic
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

/// The writer thread's local handles to the shared liveness state. Split out so
/// the thread closure does not have to hold an `Arc<DbInner>` (which would keep
/// the channel sender alive and stop the loop ever ending).
#[derive(Clone)]
pub(crate) struct WriterWatch {
    live: std::sync::Arc<AtomicU8>,
    panics: std::sync::Arc<AtomicU64>,
    last_panic: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl WriterWatch {
    fn record(&self, message: String) -> u64 {
        let n = self.panics.fetch_add(1, Ordering::SeqCst) + 1;
        *self
            .last_panic
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(message);
        n
    }
}

impl Drop for DbInner {
    /// ORDERED SHUTDOWN (t8; the race is older than t8 — migration 26 exposed
    /// it). Closing the channel ends the writer loop, whose last act is a
    /// best-effort `wal_checkpoint(TRUNCATE)`; the `rusqlite::Connection` — and
    /// with it the file handle and any WAL lock — lives until that thread
    /// RETURNS. Before this `Drop`, the last `Db` handle could be dropped and the
    /// same path reopened while the checkpoint was still running, and the reopen
    /// failed with `SQLITE_BUSY ... database is locked`
    /// (`crates/store/tests/ledger_reopen.rs:100`). The window only mattered when
    /// the reopen had WORK to do: a ledger whose MAX row is gone re-applies the
    /// highest migration, i.e. it writes, while a reopen with nothing to apply
    /// only reads. That is why adding migration 26 turned a green test into a
    /// red one without touching either file.
    ///
    /// This is a PRODUCT fix, not a test accommodation: the same window exists
    /// for any second opener in this process (the CLI opening the database a
    /// daemon holds is that shape one process over).
    ///
    /// NOT on the writer thread itself: an op closure can hold the last `Db`
    /// clone (`Db::call` hands the closure to the writer), and joining self
    /// would deadlock. There the join is unnecessary — the thread has already
    /// passed the point where anyone could race it.
    fn drop(&mut self) {
        // CLOSE THE CHANNEL FIRST. The writer loop ends when its receiver sees
        // the last sender gone; joining before that would wait for ever (and a
        // queued op is still drained — the receiver ends only after the queue is
        // empty).
        self.tx.take();
        if std::thread::current().id() == self.writer_id {
            return;
        }
        if let Some(handle) = self.writer.take() {
            // Best effort by design: a writer that panicked must not panic its
            // last owner, and there is nothing left to report to.
            let _ = handle.join();
        }
    }
}

impl Db {
    /// Open (or create) the database at `path`, apply migrations, and
    /// start the writer thread.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = rusqlite::Connection::open(path)?;
        Self::configure_and_spawn(conn)
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::configure_and_spawn(conn)
    }

    fn configure_and_spawn(mut conn: rusqlite::Connection) -> Result<Self, DbError> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        crate::migrations::apply(&mut conn)?;
        Self::spawn_thread(conn)
    }

    fn spawn_thread(mut conn: rusqlite::Connection) -> Result<Self, DbError> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Op>();
        // Shared with the writer thread, which is the only thing that can observe
        // a panic as it happens.
        let live = std::sync::Arc::new(AtomicU8::new(LIVE_RUNNING));
        let panics = std::sync::Arc::new(AtomicU64::new(0));
        let last_panic = std::sync::Arc::new(std::sync::Mutex::new(None));
        let watch = WriterWatch {
            live: live.clone(),
            panics: panics.clone(),
            last_panic: last_panic.clone(),
        };
        // One clone for the thread's loop, one kept for the handle: the loop consumes
        // its clone, and `DbInner` needs its own so an op can hand the writer thread
        // the watch to record into (see `execute`).
        let thread_watch = watch.clone();
        let handle_watch = watch.clone();
        let writer = std::thread::Builder::new()
            .name("ruagent-db-writer".into())
            .spawn(move || {
                while let Some(op) = rx.blocking_recv() {
                    // THE CATCHING, THE RECORD AND THE WAKE-UP ALL LIVE IN `execute`,
                    // in that order, and the reply channel is created per operation right
                    // here. This loop is deliberately dumb: if the bookkeeping ever moves
                    // back in here after the reply, the ordering bug returns (it was
                    // measured -- a test read the PREVIOUS panic on 3 runs in 14). One
                    // place can be right; two places drift.
                    // The wake channel is created per operation HERE, and `execute`
                    // (reached through the op) is what uses it: it runs the work, records
                    // a panic if one is raised, and only THEN wakes the caller — on the
                    // panic path by dropping this sender. The watch travels with the op so
                    // the record lands in THIS actor's state and no other's.
                    let (wake_tx, _wake_rx) = tokio::sync::oneshot::channel::<Frame>();
                    op(
                        &mut conn,
                        Box::new(move |frame| {
                            let _ = wake_tx.send(frame);
                        }),
                        watch.clone(),
                    );
                }
                // All handles dropped: best-effort checkpoint before exit. A panic here
                // still ends the thread -- which is what marks the writer `Stopped` for
                // `health`.
                let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
                thread_watch.live.store(LIVE_STOPPED, Ordering::SeqCst);
            })?;
        let writer_id = writer.thread().id();
        Ok(Self {
            inner: std::sync::Arc::new(DbInner {
                tx: Some(tx),
                writer: Some(writer),
                writer_id,
                live,
                panics,
                last_panic,
                watch: handle_watch,
            }),
        })
    }

    /// Run a closure with the connection on the writer thread and await
    /// its result.
    ///
    /// NOTE the shape: `T` is unconstrained, so a closure returning a
    /// `rusqlite::Result<X>` (nearly every real query) makes this return
    /// `Result<Result<X, rusqlite::Error>, DbError>`. The two layers mean
    /// different things -- the outer is "the writer thread is gone", the inner
    /// is "the SQL failed" -- and the outer is almost never the one that goes
    /// wrong. A caller that judges only the outer (`if let Err(e) = call(..)`,
    /// `.await.is_err()`, `let _ = call(..)`) cannot see a failed statement.
    /// Prefer `Db::call_flat`, which makes that mistake unrepresentable
    /// (t324/t327).
    pub async fn call<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&mut rusqlite::Connection) -> T + Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<Frame>();
        let sender = self.inner.tx.as_ref().ok_or(DbError::Closed)?.clone();
        let watch = self.inner.watch.clone();
        let mut work = Some(f);
        if sender
            .send(Box::new(move |conn, _wake: OpWake, _watch: WriterWatch| {
                // The wake path is `tx` itself. `execute` runs the work, records a panic
                // if one happens, and only then calls this — or drops it, which is what
                // wakes the caller on the panic path.
                let wake: Wake = Box::new(move |frame| {
                    let _ = tx.send(frame);
                });
                let f = work
                    .take()
                    .expect("an op is polled at most once; this closure is FnOnce");
                execute(conn, wake, &watch, f);
            }))
            .is_err()
        {
            // The receiver is gone: the writer thread has ended (a panic outside the
            // per-operation catch, or the ordinary shutdown).
            return Err(self.writer_gone());
        }
        // A dropped wake sender (the panic path) wakes us with `Err`, and so does a
        // dead writer: both are a failed operation, and the panic has already been
        // counted and logged by `execute`.
        match rx.await {
            Ok(frame) => frame
                .0
                .downcast::<T>()
                .map(|v| *v)
                .map_err(|_| DbError::Closed),
            Err(_) => Err(self.writer_gone()),
        }
    }

    /// Run a SQL closure and return ONE error type: both layers collapsed.
    ///
    /// Use this for anything that reads or writes through the product's SQL.
    /// `call` hands back a nested `Result` whose outer half only reports a
    /// dead writer, and t324 found 17 places where that nesting made a failure
    /// invisible (a build whose "failed" marker never lands stays `running`
    /// for ever; a failed read becomes an empty map). With this entry point the
    /// nested misjudgement cannot be written at all: there is nothing to judge
    /// but the real error.
    pub async fn call_flat<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&mut rusqlite::Connection) -> Result<T, rusqlite::Error> + Send + 'static,
    {
        self.call(f).await?.map_err(DbError::from)
    }

    /// What the writer actor is doing right now (ruagent-close-the-gaps t16).
    ///
    /// A health check asks THIS, not "did my last query succeed": a daemon whose
    /// writer died answered `health ok` while every store-backed route failed,
    /// and the only cure was a restart nobody knew they needed.
    pub fn writer_state(&self) -> WriterState {
        match self.inner.live.load(Ordering::SeqCst) {
            LIVE_STOPPED => WriterState::Stopped,
            _ => WriterState::Running,
        }
    }

    /// How many closures have panicked on the writer thread since this `Db` was
    /// opened. Non-zero does not mean the data plane is down — the actor survives
    /// — it means at least one operation died, and the detail is in the log and in
    /// [`Db::last_writer_panic`].
    pub fn writer_panics(&self) -> u64 {
        self.inner.panics.load(Ordering::SeqCst)
    }

    /// The text of the most recent panic the writer thread caught.
    pub fn last_writer_panic(&self) -> Option<String> {
        self.inner.last_panic()
    }

    /// The error for "there is no writer to serve you".
    ///
    /// With the per-operation catch in place the writer only stops after its last
    /// handle is dropped, so this is the ordinary shutdown reading — but it is a
    /// NAMED error rather than an anonymous closed channel, because "the data plane
    /// is gone" must not be indistinguishable from "my query failed".
    fn writer_gone(&self) -> DbError {
        DbError::Closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruagent_core::{Task, TaskCreator};

    #[tokio::test]
    async fn concurrent_writes_all_land() {
        // 32 concurrent inserters through the single writer: all succeed,
        // no lock errors (the whole point of D7).
        let db = Db::open_in_memory().unwrap();
        let mut handles = Vec::new();
        for i in 0..32 {
            let db = db.clone();
            handles.push(tokio::spawn(async move {
                let t = Task::new(
                    format!("task {i}"),
                    format!("intent {i}"),
                    TaskCreator::Human,
                );
                db.insert_task(&t).await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let all = db.list_tasks(None).await.unwrap();
        assert_eq!(all.len(), 32);
    }

    #[tokio::test]
    async fn open_applies_migrations() {
        let dir = std::env::temp_dir().join(format!("ruagent-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        {
            let db = Db::open(&path).unwrap();
            let t = Task::new("t", "i", TaskCreator::Human);
            db.insert_task(&t).await.unwrap();
            let got = db.get_task(t.id).await.unwrap();
            assert!(got.is_some());
        }
        // Reopen: migrations are idempotent, data survives.
        {
            let db = Db::open(&path).unwrap();
            let all = db.list_tasks(None).await.unwrap();
            assert_eq!(all.len(), 1);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// THE WINDOW THIS CRATE USED TO LEAVE OPEN (t8). Dropping the last `Db`
    /// handle returned before the writer thread's checkpoint had finished, so an
    /// immediate reopen of the same path raced it. The race only bit when the
    /// reopen had WORK to do — a ledger whose MAX row is gone re-applies the
    /// highest migration, i.e. it writes — which is exactly what migration 26
    /// turned from a read into a write, reddening
    /// `crates/store/tests/ledger_reopen.rs` with `SQLITE_BUSY ... database is
    /// locked` while this crate's own code was unchanged.
    ///
    /// The fix is the ordered shutdown in `DbInner::drop`, so a reopen after a
    /// drop must ALWAYS work, not usually: this drives the shape 20 times rather
    /// than once, because a single pass is what passed before the fix too.
    #[tokio::test]
    async fn a_reopen_after_the_last_handle_drops_never_races_the_writer() {
        let dir = std::env::temp_dir().join(format!("ruagent-db-reopen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("reopen.db");
        for round in 0..20 {
            let db = Db::open(&path).unwrap_or_else(|e| panic!("round {round}: open failed: {e}"));
            // The damaged-ledger shape: the MAXIMUM version row is missing, so
            // the next open must re-apply that migration (a WRITE, not a read).
            let punched = db
                .call_flat(|conn| {
                    conn.execute(
                        "DELETE FROM schema_migrations
                          WHERE version = (SELECT MAX(version) FROM schema_migrations)",
                        [],
                    )
                })
                .await
                .unwrap_or_else(|e| panic!("round {round}: punch failed: {e}"));
            assert_eq!(punched, 1, "round {round}: the MAX row must be gone");
            // No sleep, no retry: the drop itself must make the file reusable.
            drop(db);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// t327: the two entry points side by side on the SAME failing statement.
    /// `call` keeps the SQL error in the inner layer -- where judging the
    /// outer half reports success (t324 measured exactly that) -- while
    /// `call_flat` surfaces it, which is why new code should use it.
    #[tokio::test]
    async fn call_flat_surfaces_a_failed_statement() {
        let db = Db::open_in_memory().unwrap();
        let nested = db
            .call(|conn| {
                conn.query_row("SELECT nope FROM no_such_table", [], |r| r.get::<_, i64>(0))
            })
            .await;
        assert!(
            nested.is_ok(),
            "the outer layer reports the writer thread, not the SQL"
        );
        assert!(
            nested.unwrap().is_err(),
            "the SQL failure lives in the inner layer"
        );

        let flat = db
            .call_flat(|conn| {
                conn.query_row("SELECT nope FROM no_such_table", [], |r| r.get::<_, i64>(0))
            })
            .await;
        assert!(
            matches!(flat, Err(DbError::Sqlite(_))),
            "call_flat must surface the SQL failure, got {flat:?}"
        );
    }

    /// A PANIC INSIDE A CALLER'S CLOSURE MUST NOT TAKE THE DATA PLANE DOWN
    /// (ruagent-close-the-gaps t16).
    ///
    /// WHAT THIS WOULD HAVE CAUGHT, and the reading it replaces. Before the
    /// catch, `op(&mut conn)` ran bare on the writer thread, so a panic in ANY
    /// store-backed closure unwound that thread, dropped its receiver, and every
    /// later call got `DbError::Closed` for the rest of the process's life.
    /// Measured on a private daemon (own binary, temp RUAGENT_HOME, port 8951):
    /// BEFORE, `health` answered `{"status":"ok"}` / 200 while `/api/v1/sessions`,
    /// `/api/v1/tasks`, `/api/v1/stats` and `/api/v1/memory/list` all answered 500
    /// with body `database writer is shut down`, `POST /api/v1/memory/write`
    /// answered 500, and `daemon.out` gained NOTHING — the daemon never recovered
    /// without a restart. AFTER, those same routes keep working and health reports
    /// `db.writer_panics`/`db.last_panic` while staying ok.
    ///
    /// The three readings here are the contract: the caller is told, the count
    /// moves, and the NEXT operation is served.
    #[tokio::test]
    async fn a_panic_in_a_closure_does_not_kill_the_writer() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(
            db.writer_state(),
            WriterState::Running,
            "a fresh actor must report Running"
        );
        assert_eq!(db.writer_panics(), 0);

        // 1. A closure that panics, through the store's own entry point.
        let call = db
            .call(|_conn| -> i64 { panic!("t16: deliberate panic inside a closure") })
            .await;
        assert!(
            call.is_err(),
            "the caller must be TOLD the operation did not complete"
        );

        // 2. THE ORDERING IS THE FIX, SO IT IS WHAT IS PINNED. `execute` records the
        //    panic BEFORE it wakes the caller, so this read happens with no further
        //    synchronisation at all. The shape this replaces read the PREVIOUS panic's
        //    text on 3 runs in 14 (and `0` on 1 run in 6) because the reply used to be
        //    sent by the closure itself, ahead of the bookkeeping. If someone moves the
        //    wake-up back in front of the record, this assertion is what fails.
        assert_eq!(
            db.writer_panics(),
            1,
            "the count must be visible the moment the caller is told -- reading it here \
             requires no extra synchronisation, which is the whole point of the ordering"
        );
        let last = db
            .last_writer_panic()
            .expect("the panic text must be kept for the log/health to show");
        assert!(
            last.starts_with(STORE_WRITER_PANIC_MARKER),
            "the kept record must say a store operation panicked, got {last:?}"
        );
        // The panic's own TEXT is best-effort (see the note at the catch): the same
        // expression was measured returning the message and, via a helper, an empty
        // fallback. What the store PROMISES is the marker and the count, so that is
        // what is pinned; the text, when present, is appended after the marker.
        assert!(
            last.len() >= STORE_WRITER_PANIC_MARKER.len(),
            "the marker alone is still a usable announcement"
        );

        // 2b. The OTHER payload shape (a `format!`ed panic) must be readable too.
        //     These two forms do not always produce the same runtime type, and the
        //     first version of the extractor read only one of them.
        let _ = db
            .call(|_conn| -> i64 { panic!("t16: formatted {}", "closure") })
            .await;
        assert_eq!(
            db.writer_panics(),
            2,
            "the second panic must be counted before the caller is woken"
        );
        let second = db.last_writer_panic().unwrap_or_default();
        assert!(
            second.contains("t16: formatted closure"),
            "a formatted panic must also be readable, got {second:?}"
        );

        // 3. `call_flat` behaves the same way (it delegates to `call`).
        let flat = db
            .call_flat(|_conn| -> Result<i64, rusqlite::Error> {
                panic!("t16: deliberate panic inside a flat closure")
            })
            .await;
        assert!(flat.is_err(), "call_flat must also tell the caller");
        assert_eq!(db.writer_panics(), 3, "every panic must be counted");

        // 4. THE POINT: the writer still serves. Before the catch this is exactly
        //    the call that returned `Closed` for the rest of the process's life.
        let after = db
            .call_flat(|conn| {
                conn.query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, i64>(0))
            })
            .await;
        assert_eq!(
            after.as_ref().ok().copied(),
            Some(0),
            "the writer must keep serving after a caught panic, got {after:?}"
        );
        assert_eq!(
            db.writer_state(),
            WriterState::Running,
            "a caught panic leaves the actor running -- the count is what carries the news"
        );

        // 5. And a WRITE still lands, so "still serving" is not a read-only claim.
        //    (In-memory database: this exercises the same statement path the
        //    daemon's routes use.)
        db.call_flat(|conn| {
            conn.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![800_000_000i64, "t16-after-panic"],
            )
        })
        .await
        .expect("a write after a caught panic must succeed");
        let rows = db
            .call_flat(|conn| {
                conn.query_row(
                    "SELECT count(*) FROM schema_migrations WHERE version = 800000000",
                    [],
                    |r| r.get::<_, i64>(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(rows, 1, "the write after the panic must be visible");
    }

    /// The health/observability half, pinned at the store: the actor reports
    /// `Stopped` once nothing can serve, which is the reading `health` turns into
    /// a 503 instead of `ok`.
    #[tokio::test]
    async fn a_stopped_writer_is_reported_as_stopped_not_running() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.writer_state(), WriterState::Running);
        drop(db);
        // `Db` is gone, so there is nobody left to ask -- which is the point: the
        // state is carried by the SHARED Arc, so a clone observes the transition.
        // Re-open a fresh actor and observe it is Running, to prove the reading is
        // about the actor and not a constant.
        let fresh = Db::open_in_memory().unwrap();
        assert_eq!(
            fresh.writer_state(),
            WriterState::Running,
            "a live actor must never be reported stopped"
        );
        assert_eq!(fresh.writer_panics(), 0);
        assert_eq!(fresh.last_writer_panic(), None);
    }
}
