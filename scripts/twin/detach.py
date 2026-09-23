"""Start a command in its own session, so a group-directed SIGTERM/SIGINT
aimed at the calling shell never reaches it (robotd stops on both).

    detach.py <logfile> <cmd> [args...]

Prints the detached pid; stop it later by that pid, and only that pid.
(After sim-maploc/detach.py on the robotd fork's PR 202 branch.)
"""
import os
import sys

log, argv = sys.argv[1], sys.argv[2:]

pid = os.fork()
if pid:
    os.waitpid(pid, 0)
    sys.exit(0)

os.setsid()
pid2 = os.fork()
if pid2:
    print(pid2, flush=True)
    os._exit(0)

fd = os.open(os.devnull, os.O_RDONLY)
os.dup2(fd, 0)
out = os.open(log, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
os.dup2(out, 1)
os.dup2(out, 2)
os.execvp(argv[0], argv)
