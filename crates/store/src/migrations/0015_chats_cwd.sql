-- Project working directory for a chat (the dsh-web model: a session
-- belongs to a workspace). NULL = the daemon's per-chat scratch
-- workspace (root/workspaces/chat-<id>). Set at chat start, kept
-- across handoff/resume so a conversation stays in its project.
ALTER TABLE chats ADD COLUMN cwd TEXT;
