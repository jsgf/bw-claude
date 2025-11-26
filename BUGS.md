# Bugs and TODO

## Completed Fixes
- ✅ /etc/ is writable → Fixed: Now using bwrap's `--remount-ro /etc` after mounting essential files
- ✅ add timezone files → Fixed: Added "timezone" and "localtime" to ESSENTIAL_ETC_FILES
- ✅ working dir should be RW by default → Fixed: Changed from read-only to read-write mount
- ✅ allow relative paths for --allow-ro/rw → Fixed: Added relative path resolution relative to target_dir
- ✅ don't assume bw-relay is in /usr/local/bin → Fixed: Now searches same directory as current exe and in PATH

## Security Notes
- Consider using seccomp to disallow the TIOCSTI ioctl if not using --new-session, to prevent applications from feeding keyboard input to the terminal (CVE-2017-5226)
