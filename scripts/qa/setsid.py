import os
import sys

# setsid(1) for hosts without it (macOS): new session, then replace this process with the command.
os.setsid()
os.execvp(sys.argv[1], sys.argv[1:])
