//! Help, usage and version texts.

/// Top-level usage (`bmk -h`, and stderr for bare `bmk`).
pub const LOOKUP: &str = "\
bmk: scoped directory bookmarks

Usage:
  bmk <namespace> <name>       Print the bookmarked path (also: bmk <namespace>/<name>)
  bmk --cli <command> [args]   Manage bookmarks (short: -c)
  bmk -h, --help               Show this help
  bmk -V, --version            Show version

In fish, quote the substitution so an unknown bookmark can't send you to $HOME:
  cd \"$(bmk personal app)\"

Bookmarks file: ~/.config/bmk/bookmarks
";

/// `bmk -V`; `BMK_BUILD` comes from `build.rs`.
pub const VERSION: &str = concat!(
    "bmk ",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("BMK_BUILD"),
    ")\n"
);

/// Management overview (`bmk --cli help`).
pub const CLI: &str = "\
Usage: bmk --cli <command> [args]   (short: bmk -c <command>)

Commands:
  add <key> <path> [-t <tags>] [-f]   Add a bookmark ('.' = current directory)
  rm <key>                            Remove a bookmark
  mv <key> <new-key>                  Rename a bookmark
  tag <key> (-t <tags> | --clear)     Replace or clear a bookmark's tags
  ls [<prefix>] [-t <tags>]           List bookmarks (all given tags must match)
  tags                                List tags with usage counts
  edit                                Open the bookmarks file in $VISUAL / $EDITOR
  doctor                              Check the file and every target directory
  path                                Print the bookmarks file location
  fish                                Print the fish completion script
  help [<command>], version

<key> is <namespace>/<name> or <name>. <tags> is comma-separated: -t docker,infra
Run 'bmk --cli <command> --help' for details.
";

pub const ADD: &str = "\
Usage: bmk --cli add <key> <path> [-t|--tags <tags>] [-f|--force]

Add a bookmark. <path> may be relative ('.' is the current directory); it is
stored as an absolute path, with your home directory written as '~'.

  -t, --tags <tags>   Comma-separated tags, e.g. docker,infra
  -f, --force         Replace an existing bookmark with the same key
";
pub const RM: &str =
    "Usage: bmk --cli rm <key>\n\nRemove a bookmark. Namespaces are never removed as a whole.\n";
pub const MV: &str = "Usage: bmk --cli mv <key> <new-key>\n\nRename a bookmark, keeping its path, tags and position.\n";
pub const TAG: &str = "\
Usage: bmk --cli tag <key> (-t|--tags <tags> | --clear)

Replace a bookmark's tags, or remove them all with --clear.
";
pub const LS: &str = "\
Usage: bmk --cli ls [<prefix>] [-t|--tags <tags>]

List bookmarks in file order. <prefix> limits the list to a namespace or key;
-t keeps bookmarks that have all of the given tags. Piped output is
tab-separated (key, path, tags) with '~' expanded.
";
pub const TAGS: &str =
    "Usage: bmk --cli tags\n\nList every tag with the number of bookmarks using it.\n";
pub const EDIT: &str = "\
Usage: bmk --cli edit

Open the bookmarks file in $VISUAL, $EDITOR or vi, then check it for problems.
";
pub const DOCTOR: &str = "\
Usage: bmk --cli doctor

Check the bookmarks file for problems and verify that every target directory
exists. Exits 1 if anything is wrong.
";
pub const PATH: &str = "Usage: bmk --cli path\n\nPrint the location of the bookmarks file.\n";
pub const FISH: &str = "\
Usage: bmk --cli fish

Print the fish completion script. To install it:
  echo 'command -q bmk; and command bmk --cli fish | source' > ~/.config/fish/completions/bmk.fish
";
