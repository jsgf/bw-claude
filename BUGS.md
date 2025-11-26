# Bugs and TODO

Delete things from here when complete

## Bugs
- /etc/ is writable. Use --perms to make RO? Or --bind-ro a skeleton dir?
  - do we need /root? Why rw?
- add timezone files
- working dir should be RW by default
- allow relative paths for --allow-ro/rw (relative to . / --dir)
- don't assume bw-relay is in /usr/local/bin
  - maybe all executables should be the same and use arg[0] as 

> Note: In a general sandbox, if you don't use --new-session, it is recommended to use seccomp to disallow the TIOCSTI ioctl, otherwise the application can feed keyboard input to the terminal which can e.g. lead to out-of-sandbox command execution (see CVE-2017-5226).
