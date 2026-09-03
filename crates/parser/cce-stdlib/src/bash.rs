// Bash Standard Library Detector
// Handles detection of Bash built-in commands and common utilities

pub struct BashStdlibDetector;

impl BashStdlibDetector {
    // Bash built-in commands (MUST be sorted)
    pub const BUILTIN_COMMANDS: &[&str] = &[
        "begin", "bg", "break", "builtin", "case", "cd", "command", "continue", "declare",
        "disown", "do", "done", "echo", "else", "elif", "enable", "end", "esac", "exec", "exit",
        "export", "fi", "fg", "for", "function", "help", "if", "info", "jobs", "kill", "let",
        "local", "man", "printf", "read", "return", "set", "shopt", "suspend", "tee", "then",
        "trap", "type", "typeset", "unalias", "unset", "wait", "while", "{", "}", "(", ")",
    ];

    // Common standard utilities (POSIX/GNU coreutils - MUST be sorted)
    pub const STANDARD_UTILITIES: &[&str] = &[
        "7z",
        "a2ps",
        "adduser",
        "alias",
        "apt",
        "apt-get",
        "awk",
        "bg",
        "bunzip2",
        "bzip2",
        "c++",
        "cal",
        "cargo",
        "cat",
        "cc",
        "chgrp",
        "chmod",
        "chown",
        "cmake",
        "cmp",
        "comm",
        "compgen",
        "complete",
        "compopt",
        "curl",
        "cut",
        "cvs",
        "date",
        "deluser",
        "diff",
        "dig",
        "dmesg",
        "dnf",
        "docker",
        "docker-compose",
        "dpkg",
        "du",
        "ed",
        "emacs",
        "enscript",
        "exitcode",
        "fgrep",
        "find",
        "finger",
        "free",
        "ftp",
        "g++",
        "gcc",
        "gem",
        "git",
        "gpg",
        "grep",
        "groups",
        "gunzip",
        "gzip",
        "head",
        "hg",
        "host",
        "hostname",
        "htop",
        "hwclock",
        "id",
        "ifconfig",
        "info",
        "iwconfig",
        "join",
        "journalctl",
        "kubectl",
        "ldconfig",
        "less",
        "lftp",
        "ln",
        "locate",
        "lp",
        "lpstat",
        "ls",
        "lsmod",
        "lspci",
        "lsusb",
        "make",
        "man",
        "md5sum",
        "mercurial",
        "mkdir",
        "modprobe",
        "mongo",
        "more",
        "mount",
        "mv",
        "mysql",
        "nc",
        "ncat",
        "nice",
        "nohup",
        "npm",
        "nslookup",
        "objdump",
        "openssl",
        "pacman",
        "passwd",
        "paste",
        "patch",
        "pdb",
        "pg_dump",
        "pgp",
        "pgrep",
        "pid",
        "pip",
        "pkill",
        "podman",
        "ps",
        "psql",
        "pwd",
        "rar",
        "readelf",
        "redis-cli",
        "rename",
        "renice",
        "rm",
        "rmdir",
        "rpm",
        "rsync",
        "ruby",
        "scp",
        "screen",
        "sed",
        "sha1sum",
        "sha256sum",
        "sha512sum",
        "socat",
        "sort",
        "split",
        "sqlite3",
        "ss",
        "ssh",
        "stat",
        "strings",
        "strip",
        "su",
        "sudo",
        "svn",
        "tar",
        "telnet",
        "timedatectl",
        "tmux",
        "top",
        "touch",
        "tr",
        "trace",
        "traceroute",
        "unalias",
        "uname",
        "uniq",
        "unset",
        "unzip",
        "uptime",
        "vagrant",
        "vboxmanage",
        "vi",
        "view",
        "vim",
        "virsh",
        "w",
        "wc",
        "wget",
        "whereis",
        "which",
        "whoami",
        "xargs",
        "xdg-open",
        "xz",
        "yum",
        "zip",
    ];

    /// Check if a command is a Bash built-in command
    pub fn is_builtin_command(cmd: &str) -> bool {
        Self::BUILTIN_COMMANDS.binary_search(&cmd).is_ok()
    }

    /// Check if a command is a standard POSIX/GNU utility
    pub fn is_standard_utility(cmd: &str) -> bool {
        Self::STANDARD_UTILITIES.binary_search(&cmd).is_ok()
    }

    /// Check if a call is to stdlib (built-in command or standard utility)
    pub fn is_stdlib_call(call_name: &str) -> bool {
        Self::is_builtin_command(call_name) || Self::is_standard_utility(call_name)
    }

    /// Check if a call is to stdlib using relation type (optimized)
    pub fn is_stdlib_by_type(
        call_name: &str,
        relation_type: &cce_types::relation::RelationType,
    ) -> bool {
        use cce_types::relation::RelationType;

        match relation_type {
            // Only direct calls are relevant for command detection
            RelationType::DirectCall => Self::is_stdlib_call(call_name),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin_command() {
        assert!(BashStdlibDetector::is_builtin_command("echo"));
        assert!(BashStdlibDetector::is_builtin_command("if"));
        assert!(BashStdlibDetector::is_builtin_command("for"));
        assert!(!BashStdlibDetector::is_builtin_command("my_command"));
    }

    #[test]
    fn test_is_standard_utility() {
        assert!(BashStdlibDetector::is_standard_utility("grep"));
        assert!(BashStdlibDetector::is_standard_utility("sed"));
        assert!(BashStdlibDetector::is_standard_utility("awk"));
        assert!(BashStdlibDetector::is_standard_utility("find"));
        assert!(!BashStdlibDetector::is_standard_utility("my_utility"));
    }

    #[test]
    fn test_is_stdlib_call() {
        assert!(BashStdlibDetector::is_stdlib_call("echo"));
        assert!(BashStdlibDetector::is_stdlib_call("grep"));
        assert!(BashStdlibDetector::is_stdlib_call("for"));
        assert!(!BashStdlibDetector::is_stdlib_call("my_function"));
    }
}
