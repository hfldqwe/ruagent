#!/usr/bin/env python3
"""Workflow-file sanity that GitHub only tells you about AFTER a run is created.

Measured failure this exists to prevent (v0.1.8, commit 1e71986): a job's `if` read
`needs.gate.result` while its `needs` listed only `build`. GitHub then refused the whole file --
the tag push created a run with NO JOBS and the text 'This run likely failed because of a workflow
file issue', which reads like an infrastructure problem and publishes nothing. YAML parsing does not
catch it; neither does `python -c yaml.safe_load`. Two checks do:

  1. no duplicate mapping keys (a second `if:` silently wins over the first);
  2. every `needs.<job>` reference names a job that is in that job's own `needs`.

Exit 1 and name the file, the job and the reference. No third-party packages.
"""
import sys
import pathlib
import re

import yaml


class StrictLoader(yaml.SafeLoader):
    pass


def _no_duplicates(loader, node, deep=False):
    seen = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in seen:
            raise ValueError(f'duplicate key {key!r} at line {key_node.start_mark.line + 1}')
        seen[key] = loader.construct_object(value_node, deep=deep)
    return seen


StrictLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _no_duplicates
)

NEEDS_REF = re.compile(r'needs\.([A-Za-z0-9_-]+)')


def walk(node):
    if isinstance(node, str):
        yield node
    elif isinstance(node, dict):
        for v in node.values():
            yield from walk(v)
    elif isinstance(node, list):
        for v in node:
            yield from walk(v)


def check(path):
    problems = []
    text = path.read_text(encoding='utf-8')
    try:
        doc = yaml.load(text, Loader=StrictLoader)
    except Exception as exc:
        return [f'{path}: not parseable: {exc}']
    jobs = (doc or {}).get('jobs') or {}
    for name, job in jobs.items():
        needs = job.get('needs') or []
        if isinstance(needs, str):
            needs = [needs]
        for dep in needs:
            if dep not in jobs:
                problems.append(f'{path}: job {name!r} needs {dep!r}, which is not a job here')
        for expr in walk(job):
            for ref in NEEDS_REF.findall(expr):
                if ref not in jobs:
                    problems.append(
                        f'{path}: job {name!r} refers to needs.{ref}, which is not a job here'
                    )
                elif ref not in needs:
                    problems.append(
                        f'{path}: job {name!r} refers to needs.{ref} but its needs list is {needs} '
                        '-- GitHub rejects the WHOLE file for this (a run with no jobs)'
                    )
    return problems


def main(argv):
    root = pathlib.Path(argv[1] if len(argv) > 1 else '.github/workflows')
    files = sorted(p for p in root.rglob('*.yml')) + sorted(p for p in root.rglob('*.yaml'))
    print(f"workflow files checked: {len(files)}")
    problems = []
    for path in files:
        problems.extend(check(path))
    for problem in problems:
        print(f'  FAIL {problem}')
    if problems:
        print(f'{len(problems)} problem(s)')
        return 1
    print("all workflow files parse strictly, and every needs.<job> reference is declared")
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))