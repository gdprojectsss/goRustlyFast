#!/usr/bin/env python3
import argparse
import concurrent.futures
import json
import os
import re
import shutil
import signal
import subprocess
import sys

EXCLUDED_SUITES = {
    "interactive",
    "interactive-parse",
    "history-expand",
    "builtin-bind",
}

KEY_VALUE = re.compile(
    r"""
    [#][#] \s+
    (?: (OK(?:-\d)? | BUG(?:-\d)? | N-I) \s+ ([\w+/]+) \s+ )?
    ([\w\-]+)
    :
    \s* (.*)
    """,
    re.VERBOSE,
)
END_BLOCK = re.compile(r"[#][#] \s+ END", re.VERBOSE)

CASE_BEGIN, METADATA, BLOCK_BEGIN, BLOCK_END, CODE, END_OF_FILE = range(6)

PASSING = {"PASS", "OK", "BUG", "N-I"}

ARGV_SHIM = r"""#!/usr/bin/env python3
import os, sys
def python2_repr(raw):
    text = repr(raw)
    return text[1:]
print("[" + ", ".join(python2_repr(os.fsencode(argument)) for argument in sys.argv[1:]) + "]")
"""

PRINTENV_SHIM = r"""#!/usr/bin/env python3
import os, sys
for name in sys.argv[1:]:
    value = os.environb.get(os.fsencode(name))
    sys.stdout.buffer.write((b"None" if value is None else value) + b"\n")
"""

READ_FROM_FD_SHIM = r"""#!/usr/bin/env python3
import os, sys
for argument in sys.argv[1:]:
    descriptor = int(argument)
    try:
        data = os.read(descriptor, 1024)
    except OSError as error:
        print("FATAL: Error reading from fd %d: %s" % (descriptor, error), file=sys.stderr)
        sys.exit(1)
    sys.stdout.buffer.write(b"%d: " % descriptor + data)
    sys.stdout.buffer.flush()
"""

PYTHON2_SHIM = r"""#!/usr/bin/env python3
import builtins, os, re, sys


class Python2Text(str):
    def decode(self, encoding="utf-8"):
        return os.fsencode(self).decode(encoding)

    def __repr__(self):
        return builtins_repr(os.fsencode(self))[1:]


def encode(value):
    if isinstance(value, bytes):
        return value
    text = value if isinstance(value, str) else str(value)
    try:
        return text.encode("latin-1")
    except UnicodeEncodeError:
        return text.encode("utf-8", "surrogateescape")


def python2_print(*values, sep=" ", end="\n", file=None):
    stream = file or sys.stdout
    data = encode(sep).join(encode(value) for value in values) + encode(end)
    target = getattr(stream, "buffer", None)
    if target is None:
        stream.write(data.decode("latin-1"))
    else:
        stream.flush()
        target.write(data)
        target.flush()


def python2_repr(value):
    if isinstance(value, bytes):
        return builtins_repr(value)[1:]
    return builtins_repr(value)


builtins_repr = builtins.repr
arguments = sys.argv[1:]
if arguments[:1] != ["-c"]:
    os.execvp("python3", ["python3"] + arguments)
code = re.sub(r"(^|;)([ \t]*)print[ \t]+(?!\()([^;\n]+)", r"\1\2print(\3)", arguments[1], flags=re.MULTILINE)
code = code.replace("from __future__ import print_function", "")
sys.argv = ["-c"] + [Python2Text(argument) for argument in arguments[2:]]
builtins.print = python2_print
builtins.repr = python2_repr
exec(compile(code, "<string>", "exec"), {"__name__": "__main__"})
sys.stdout.flush()
"""


class Lines:
    def __init__(self, path):
        with open(path, encoding="utf-8", errors="surrogateescape") as handle:
            self.lines = handle.readlines()
        self.position = 0
        self.number = 0
        self.current = None
        self.advance()

    def classify(self, line, raw):
        if line == "":
            return (self.number, END_OF_FILE, "")
        if not raw and not line.strip():
            return None
        if line.startswith("####"):
            return (self.number, CASE_BEGIN, line[4:].strip())
        match = KEY_VALUE.match(line)
        if match:
            qualifier, shells, name, value = match.groups()
            if name in ("stdout", "stderr"):
                value += "\n"
            kind = BLOCK_BEGIN if name in ("STDOUT", "STDERR") else METADATA
            return (self.number, kind, (qualifier, shells, name, value))
        if END_BLOCK.match(line):
            return (self.number, BLOCK_END, None)
        if line.lstrip().startswith("#"):
            return None
        return (self.number, CODE, line)

    def advance(self, raw=False):
        while True:
            line = self.lines[self.position] if self.position < len(self.lines) else ""
            self.position += 1
            self.number += 1
            token = self.classify(line, raw)
            if token is not None:
                self.current = token
                return token


def add_qualified(case, qualifier, shells, name, value):
    for shell in shells.split("/"):
        entry = case.setdefault(shell, {})
        entry[name] = value
        entry["qualifier"] = qualifier


def parse_metadata(lines, case):
    while True:
        _, kind, item = lines.current
        if kind == BLOCK_BEGIN:
            qualifier, shells, name, _ = item
            collected = []
            while True:
                lines.advance(raw=True)
                _, inner_kind, inner_item = lines.current
                if inner_kind != CODE:
                    break
                collected.append(inner_item)
            value = "".join(collected)
            name = name.lower()
            if qualifier:
                add_qualified(case, qualifier, shells, name, value)
            else:
                case[name] = value
            if inner_kind == BLOCK_END:
                lines.advance()
        elif kind == METADATA:
            qualifier, shells, name, value = item
            if qualifier:
                add_qualified(case, qualifier, shells, name, value)
            else:
                case[name] = value
            lines.advance()
        else:
            return


def parse_code(lines, case):
    collected = []
    while lines.current[1] == CODE:
        collected.append(lines.current[2])
        lines.advance(raw=True)
    case["code"] = "".join(collected)


def parse_file(path):
    lines = Lines(path)
    metadata = {}
    while lines.current[1] == METADATA:
        _, _, name, value = lines.current[2]
        metadata[name] = value
        lines.advance()
    cases = []
    while lines.current[1] != END_OF_FILE:
        line_number, kind, description = lines.current
        if kind != CASE_BEGIN:
            raise RuntimeError("%s:%d: expected a #### line" % (path, line_number))
        lines.advance()
        case = {"desc": description, "line": line_number}
        parse_metadata(lines, case)
        if "code" not in case:
            if lines.current[1] != CODE:
                raise RuntimeError("%s:%d: case has no code" % (path, line_number))
            parse_code(lines, case)
            parse_metadata(lines, case)
        cases.append(case)
    return metadata, cases


def string_assertions(source, key, qualifier, assertions):
    found = False
    if key in source:
        assertions.append((key, source[key], qualifier))
        found = True
    if key + "-json" in source:
        assertions.append((key, json.loads(source[key + "-json"]), qualifier))
        found = True
    return found


def status_assertion(source, qualifier, assertions):
    if "status" in source:
        assertions.append(("status", int(source["status"]), qualifier))
        return True
    return False


def assertions_for(case, shell_label):
    assertions = []
    stdout = stderr = status = False
    if shell_label and shell_label in case:
        specific = case[shell_label]
        qualifier = specific.get("qualifier")
        stdout = string_assertions(specific, "stdout", qualifier, assertions)
        stderr = string_assertions(specific, "stderr", qualifier, assertions)
        status = status_assertion(specific, qualifier, assertions)
    if not stdout:
        string_assertions(case, "stdout", None, assertions)
    if not stderr:
        string_assertions(case, "stderr", None, assertions)
    if not status and not status_assertion(case, None, assertions):
        assertions.append(("status", 0, None))
    return assertions


def verdict(case, shell_label, actual):
    if actual["status"] == "TIMEOUT":
        return "TIMEOUT"
    ranking = ["FAIL", "BUG", "N-I", "OK", "PASS"]
    result = "PASS"
    for key, expected, qualifier in assertions_for(case, shell_label):
        if actual[key] != expected:
            return "FAIL"
        label = re.sub(r"-\d$", "", qualifier) if qualifier else "PASS"
        if ranking.index(label) < ranking.index(result):
            result = label
    if "Traceback (most recent" in actual["stderr"]:
        return "FAIL"
    return result


def make_shims(oils, work):
    shims = os.path.join(work, "bin")
    shutil.rmtree(shims, ignore_errors=True)
    os.makedirs(shims)
    replacements = {
        "argv.py": ARGV_SHIM,
        "printenv.py": PRINTENV_SHIM,
        "read_from_fd.py": READ_FROM_FD_SHIM,
    }
    with open(os.path.join(shims, "python2"), "w", encoding="utf-8") as handle:
        handle.write(PYTHON2_SHIM)
    os.chmod(os.path.join(shims, "python2"), 0o755)
    source_directory = os.path.join(oils, "spec", "bin")
    for name in os.listdir(source_directory):
        target = os.path.join(shims, name)
        if name in replacements:
            content = replacements[name]
        else:
            with open(os.path.join(source_directory, name), encoding="utf-8") as handle:
                content = handle.read().replace("#!/usr/bin/env python2", "#!/usr/bin/env python3", 1)
        with open(target, "w", encoding="utf-8") as handle:
            handle.write(content)
        os.chmod(target, 0o755)
    return shims


def stop(process):
    for target in (lambda: os.killpg(process.pid, signal.SIGKILL), process.kill):
        try:
            target()
        except OSError:
            pass


def run_case(shell_path, code, directory, environment, timeout):
    shutil.rmtree(directory, ignore_errors=True)
    os.makedirs(directory)
    environment = dict(environment, SH=shell_path, TMP=directory, HOME=directory)
    process = subprocess.Popen(
        [shell_path],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=directory,
        env=environment,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(code.encode("utf-8", "surrogateescape"), timeout=timeout)
        status = process.returncode
    except subprocess.TimeoutExpired:
        stop(process)
        try:
            stdout, stderr = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            stdout, stderr = b"", b""
        status = "TIMEOUT"
    stop(process)
    shutil.rmtree(directory, ignore_errors=True)
    if isinstance(status, int) and status < 0:
        status = 128 - status
    return {
        "stdout": stdout.decode("utf-8", "surrogateescape"),
        "stderr": stderr.decode("utf-8", "surrogateescape"),
        "status": status,
    }


def grfsh_passes(case, grfsh, bash):
    if verdict(case, "bash", grfsh) in PASSING:
        return True
    if verdict(case, None, grfsh) == "PASS":
        return True
    return bash["status"] != "TIMEOUT" and grfsh["stdout"] == bash["stdout"] and grfsh["status"] == bash["status"]


def select_suites(oils, requested):
    spec = os.path.join(oils, "spec")
    if requested:
        return requested
    suites = []
    for name in sorted(os.listdir(spec)):
        if not name.endswith(".test.sh"):
            continue
        suite = name[: -len(".test.sh")]
        if suite in EXCLUDED_SUITES or suite.startswith("ysh"):
            continue
        with open(os.path.join(spec, name), encoding="utf-8", errors="surrogateescape") as handle:
            header = handle.read(4096)
        match = re.search(r"^## compare_shells:(.*)$", header, re.MULTILINE)
        if match and {"bash", "bash-4.4"} & set(match.group(1).split()):
            suites.append(suite)
    return suites


def read_list(path):
    entries = set()
    if not path or not os.path.exists(path):
        return entries
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line and not line.startswith("#"):
                entries.add(line.split()[0])
    return entries


def case_identifier(suite, index):
    return "%s/%d" % (suite, index)


def main():
    parser = argparse.ArgumentParser(description="Run the Oils spec tests against grfsh and bash")
    parser.add_argument("--oils", required=True)
    parser.add_argument("--work", required=True)
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser.add_argument("--grfsh", default=os.environ.get("GRFSH", os.path.join(root, "target", "release", "grfsh")))
    default_bash = os.environ.get("BASH_REFERENCE") or ("/opt/homebrew/bin/bash" if os.path.exists("/opt/homebrew/bin/bash") else "bash")
    parser.add_argument("--bash", default=default_bash)
    parser.add_argument("--suites", default="", help="comma separated suite names (default: every bash suite)")
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--failures", help="write failing case ids here (default: WORK/failures.txt)")
    parser.add_argument("--details", help="write JSON lines with code, expectations and outputs of failing cases")
    parser.add_argument("--baseline", help="fail if a case outside this list of known failures fails")
    parser.add_argument("--write-baseline", help="write the failing case ids to this file")
    parser.add_argument("--quiet", action="store_true", help="do not print the list of failing cases")
    options = parser.parse_args()

    grfsh = os.path.abspath(options.grfsh) if os.sep in options.grfsh else shutil.which(options.grfsh)
    bash = shutil.which(options.bash) or options.bash
    if not grfsh or not os.access(grfsh, os.X_OK):
        sys.exit("grfsh binary not found: %s (run cargo build --release)" % options.grfsh)
    version = subprocess.run([bash, "-c", "echo ${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}"], capture_output=True, text=True).stdout.strip()
    if not version or int(version.split(".")[0]) < 5:
        print("warning: %s is bash %s; the expectations are for bash 5" % (bash, version or "?"), file=sys.stderr)

    os.makedirs(options.work, exist_ok=True)
    shims = make_shims(options.oils, options.work)
    environment = {
        "PATH": shims + os.pathsep + os.environ.get("PATH", "/usr/bin:/bin"),
        "LC_ALL": "C.UTF-8",
        "LOCALE_ARCHIVE": "",
        "OILS_GC_ON_EXIT": "",
        "REPO_ROOT": options.oils,
    }

    suites = select_suites(options.oils, [name for name in options.suites.split(",") if name])
    jobs = []
    for suite in suites:
        _, cases = parse_file(os.path.join(options.oils, "spec", suite + ".test.sh"))
        for index, case in enumerate(cases):
            jobs.append((suite, index, case))

    scratch = os.path.join(options.work, "cases")
    shutil.rmtree(scratch, ignore_errors=True)

    def run_pair(job):
        suite, index, case = job
        base = os.path.join(scratch, "%s-%03d" % (suite, index))
        bash_result = run_case(bash, case["code"], base + "-bash", environment, options.timeout)
        grfsh_result = run_case(grfsh, case["code"], base + "-grfsh", environment, options.timeout)
        if not grfsh_passes(case, grfsh_result, bash_result):
            grfsh_result = run_case(grfsh, case["code"], base + "-grfsh", environment, options.timeout)
        return suite, index, case, bash_result, grfsh_result

    totals = {}
    failures = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=options.jobs) as pool:
        for suite, index, case, bash_result, grfsh_result in pool.map(run_pair, jobs):
            counts = totals.setdefault(suite, {"cases": 0, "bash": 0, "grfsh": 0, "timeout": 0})
            counts["cases"] += 1
            if verdict(case, "bash", bash_result) in PASSING:
                counts["bash"] += 1
            if grfsh_passes(case, grfsh_result, bash_result):
                counts["grfsh"] += 1
            else:
                if grfsh_result["status"] == "TIMEOUT":
                    counts["timeout"] += 1
                failures.append((suite, index, case, bash_result, grfsh_result))
    shutil.rmtree(scratch, ignore_errors=True)

    width = max([len(suite) for suite in totals] + [5])
    print("%-*s %6s %6s %6s %8s" % (width, "suite", "cases", "bash", "grfsh", "timeouts"))
    for suite in suites:
        counts = totals.get(suite)
        if counts:
            print("%-*s %6d %6d %6d %8d" % (width, suite, counts["cases"], counts["bash"], counts["grfsh"], counts["timeout"]))
    all_cases = sum(counts["cases"] for counts in totals.values())
    bash_passed = sum(counts["bash"] for counts in totals.values())
    grfsh_passed = sum(counts["grfsh"] for counts in totals.values())
    print("%-*s %6d %6d %6d %8d" % (width, "total", all_cases, bash_passed, grfsh_passed, sum(counts["timeout"] for counts in totals.values())))
    if all_cases:
        print("bash passes %.1f%%, grfsh passes %.1f%% of %d cases" % (100.0 * bash_passed / all_cases, 100.0 * grfsh_passed / all_cases, all_cases))

    lines = ["%s\t%s" % (case_identifier(suite, index), case["desc"]) for suite, index, case, _, _ in failures]
    failures_path = options.failures or os.path.join(options.work, "failures.txt")
    with open(failures_path, "w", encoding="utf-8") as handle:
        handle.write("".join(line + "\n" for line in lines))
    if options.write_baseline:
        with open(options.write_baseline, "w", encoding="utf-8") as handle:
            handle.write("# Oils spec cases grfsh fails, regenerate with scripts/spec-tests.sh --write-baseline scripts/spec-baseline.txt\n")
            handle.write("".join(line + "\n" for line in lines))
    if options.details:
        with open(options.details, "w", encoding="utf-8") as handle:
            for suite, index, case, bash_result, grfsh_result in failures:
                record = {
                    "id": case_identifier(suite, index),
                    "desc": case["desc"],
                    "line": case["line"],
                    "code": case["code"],
                    "expected": assertions_for(case, "bash"),
                    "bash": bash_result,
                    "grfsh": grfsh_result,
                }
                handle.write(json.dumps(record, ensure_ascii=False) + "\n")
    if not options.quiet:
        for line in lines:
            print("FAIL\t" + line)
    print("failing cases written to %s" % failures_path)

    if options.baseline:
        known = read_list(options.baseline)
        failing = {case_identifier(suite, index) for suite, index, _, _, _ in failures}
        regressions = sorted(failing - known)
        fixed = sorted(known - failing)
        for identifier in fixed:
            print("now passing (remove from the baseline): %s" % identifier)
        for identifier in regressions:
            print("REGRESSION: %s" % identifier)
        if regressions:
            print("%d spec cases fail that are not in %s" % (len(regressions), options.baseline))
            return 1
        print("no regressions against %s (%d known failures, %d now passing)" % (options.baseline, len(known), len(fixed)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
