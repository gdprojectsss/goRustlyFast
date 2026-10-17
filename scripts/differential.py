#!/usr/bin/env python3
import argparse
import concurrent.futures
import glob
import os
import shutil
import signal
import subprocess
import sys
import tempfile

SCRIPT_ARGUMENTS = ["a1", "a 2"]


def load_cases(directory):
    cases = []
    for path in sorted(glob.glob(os.path.join(directory, "*.sh"))):
        stem = os.path.splitext(os.path.basename(path))[0]
        with open(path, encoding="utf-8", errors="surrogateescape") as handle:
            chunks = handle.read().split("\n#---\n")
        for index, chunk in enumerate(chunks, 1):
            code = chunk.strip("\n")
            if code.strip():
                cases.append(("%s:%d" % (stem, index), "code", code))
    for path in sorted(glob.glob(os.path.join(directory, "scripts", "*.sh"))):
        stem = os.path.splitext(os.path.basename(path))[0]
        cases.append(("scripts/%s" % stem, "script", os.path.abspath(path)))
    return cases


def read_list(path):
    entries = set()
    if path and os.path.exists(path):
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if line and not line.startswith("#"):
                    entries.add(line.split()[0])
    return entries


def stop(process):
    for target in (lambda: os.killpg(process.pid, signal.SIGKILL), process.kill):
        try:
            target()
        except OSError:
            pass


def run(shell, kind, payload, timeout):
    directory = tempfile.mkdtemp(prefix="grfsh-differential-")
    directory = os.path.realpath(directory)
    work = os.path.join(directory, "work")
    home = os.path.join(directory, "home")
    os.makedirs(work)
    os.makedirs(home)
    environment = dict(os.environ, HOME=home, LC_ALL=os.environ.get("LC_ALL", "en_US.UTF-8"))
    environment.pop("BASH_ENV", None)
    environment.pop("ENV", None)
    argv = [shell, "-c", payload, "sh"] if kind == "code" else [shell, payload] + SCRIPT_ARGUMENTS
    process = subprocess.Popen(
        argv,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=work,
        env=environment,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout)
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
    text = stdout.decode("utf-8", "surrogateescape").replace(work, "<WORK>").replace(home, "<HOME>")
    return {"status": status, "stdout": text, "stderr": stderr.decode("utf-8", "replace").strip()}


def same(first, second):
    return (
        first["status"] == second["status"]
        and first["stdout"] == second["stdout"]
        and bool(first["stderr"]) == bool(second["stderr"])
    )


def compare(bash, grfsh, case, timeout):
    identifier, kind, payload = case
    reference = run(bash, kind, payload, timeout)
    actual = run(grfsh, kind, payload, timeout)
    if not same(reference, actual):
        reference = run(bash, kind, payload, timeout)
        actual = run(grfsh, kind, payload, timeout)
    return identifier, kind, payload, reference, actual


def describe(identifier, kind, payload, reference, actual):
    lines = ["=" * 70, identifier]
    if kind == "code":
        lines.append(payload if len(payload) < 600 else payload[:600] + "...")
    lines.append("-- bash  status=%s stdout=%r stderr=%r" % (reference["status"], reference["stdout"][:300], reference["stderr"][:200]))
    lines.append("-- grfsh status=%s stdout=%r stderr=%r" % (actual["status"], actual["stdout"][:300], actual["stderr"][:200]))
    return "\n".join(lines)


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser = argparse.ArgumentParser(description="Compare grfsh with bash 5 on the differential corpus")
    parser.add_argument("--grfsh", default=os.environ.get("GRFSH", os.path.join(root, "target", "release", "grfsh")))
    default_bash = os.environ.get("BASH_REFERENCE") or ("/opt/homebrew/bin/bash" if os.path.exists("/opt/homebrew/bin/bash") else "bash")
    parser.add_argument("--bash", default=default_bash)
    parser.add_argument("--cases", default=os.path.join(root, "tests", "differential"))
    parser.add_argument("--allowlist", default=os.path.join(root, "tests", "differential", "allowlist.txt"))
    parser.add_argument("--baseline", default=os.path.join(root, "tests", "differential", "known-failures.txt"))
    parser.add_argument("--write-baseline", action="store_true", help="rewrite the known failures file from this run")
    parser.add_argument("--only", default="", help="comma separated case id prefixes to run")
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--verbose", action="store_true", help="show known and allowed differences too")
    options = parser.parse_args()

    grfsh = os.path.abspath(options.grfsh) if os.sep in options.grfsh else shutil.which(options.grfsh)
    bash = shutil.which(options.bash) or options.bash
    if not grfsh or not os.access(grfsh, os.X_OK):
        sys.exit("grfsh binary not found: %s (run cargo build --release)" % options.grfsh)
    version = subprocess.run([bash, "-c", "echo ${BASH_VERSINFO[0]}"], capture_output=True, text=True).stdout.strip()
    if version < "5":
        print("warning: %s is not bash 5 (BASH_VERSINFO=%s)" % (bash, version or "?"), file=sys.stderr)

    cases = load_cases(options.cases)
    prefixes = [prefix for prefix in options.only.split(",") if prefix]
    if prefixes:
        cases = [case for case in cases if any(case[0].startswith(prefix) for prefix in prefixes)]
    allowed = read_list(options.allowlist)
    known = read_list(options.baseline)

    differences = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=options.jobs) as pool:
        for result in pool.map(lambda case: compare(bash, grfsh, case, options.timeout), cases):
            if not same(result[3], result[4]):
                differences.append(result)

    failing = [result for result in differences if result[0] not in allowed]
    regressions = [result for result in failing if result[0] not in known]
    for result in differences:
        if result in regressions or options.verbose:
            print(describe(*result))
    fixed = sorted(known - {result[0] for result in failing})
    for identifier in fixed:
        print("now matching bash (remove from the known failures): %s" % identifier)

    print("%d cases, %d match bash, %d allowed differences, %d known failures, %d regressions" % (
        len(cases),
        len(cases) - len(differences),
        len([result for result in differences if result[0] in allowed]),
        len(failing) - len(regressions),
        len(regressions),
    ))
    if options.write_baseline:
        with open(options.baseline, "w", encoding="utf-8") as handle:
            handle.write("# Differential cases where grfsh still differs from bash, regenerate with scripts/differential.sh --write-baseline\n")
            for result in sorted(failing, key=lambda result: result[0]):
                handle.write(result[0] + "\n")
        print("wrote %s" % options.baseline)
        return 0
    return 1 if regressions else 0


if __name__ == "__main__":
    sys.exit(main())
