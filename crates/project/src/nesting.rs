//! The compositions a composition shows with `<composition src>` (`docs/spec/notation.md` section 10).
//!
//! The walk starts at the root and follows each element in document order. A file is read and judged
//! once, and its problems are reported once, under the name of the first element that names it. A file
//! that shows itself, directly or through the files it shows, is reported where the loop closes and is
//! not followed. A chain that would pass 16 files, the root counted, is reported where it would pass.
//! The walk goes on past a file with problems of its own, so that one run reports every problem.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use notation::{Code, Problem, Root, Shows, Tree};

use crate::{reason, written_path, Report};

/// The most files in one chain, the root counted (`docs/spec/notation.md` section 10).
pub const MAX_CHAIN_DEPTH: usize = 16;

/// A composition shown from another file: read and judged without a problem, with its root and its
/// elements as they are written.
#[derive(Clone, Debug)]
pub struct ShownFile {
    name: String,
    path: PathBuf,
    root: Root,
    tree: Tree,
}

impl ShownFile {
    /// The file's path as the first element that names it writes it: from the project folder, with `/`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The file's path on the machine, as [`crate::Project::resolve`] gives it for the name.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The root of the file, with the defaults filled in.
    pub fn root(&self) -> &Root {
        &self.root
    }

    /// The elements of the file as they are written.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

/// What the walk found: each file shown, read and judged without a problem, in the order the files are
/// reached, and every problem of the chain, in the order they are found.
#[derive(Debug)]
pub struct Walk {
    pub files: Vec<ShownFile>,
    pub reports: Vec<Report>,
}

/// Walks the files the root composition shows. `root_name` is the root's path from the project folder,
/// written with `/`, and `root` is what the root shows. The root's own problems are the caller's.
pub fn walk(folder: &Path, root_name: &str, root: &Shows) -> Walk {
    let mut walker = Walker {
        folder,
        cache: HashMap::new(),
        walked: HashSet::new(),
        seen: HashSet::new(),
        files: Vec::new(),
        reports: Vec::new(),
    };
    let mut chain = vec![Link {
        path: written_path(folder, root_name),
        name: root_name.to_string(),
    }];
    walker.walk(&mut chain, root);
    Walk {
        files: walker.files,
        reports: walker.reports,
    }
}

/// A file on the chain from the root to the element being walked.
#[derive(Clone, Debug)]
struct Link {
    path: PathBuf,
    name: String,
}

/// The outcome of reaching a file that an element names.
enum Reached {
    Missing,
    Unreadable(String),
    /// The file was read: its name as first written, what it shows, and its problems when this is the
    /// first time it is reached (empty otherwise).
    Found {
        name: String,
        shows: Shows,
        problems: Vec<Problem>,
    },
}

struct Walker<'a> {
    folder: &'a Path,
    /// Every file read so far, with the name it was first reached by and what it shows.
    cache: HashMap<PathBuf, (String, Shows)>,
    /// The files whose elements have been walked, with the depth they were walked at.
    walked: HashSet<(PathBuf, usize)>,
    /// The reports made, by file, line, code and the file named: one element gives one report of each.
    seen: HashSet<(PathBuf, u32, Code, PathBuf)>,
    files: Vec<ShownFile>,
    reports: Vec<Report>,
}

impl Walker<'_> {
    /// Walks the elements of the file at the end of `chain`, whose contents are `shows`.
    fn walk(&mut self, chain: &mut Vec<Link>, shows: &Shows) {
        let Some(owner) = chain.last().cloned() else {
            return;
        };
        for show in &shows.elements {
            let target = written_path(self.folder, &show.src);
            if let Some(at) = chain.iter().position(|link| link.path == target) {
                let mut names: Vec<String> = chain[at..].iter().map(|link| link.name.clone()).collect();
                names.push(chain[at].name.clone());
                let listed = names.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(", ");
                let message = format!(
                    "`{}` shows itself, directly or through the files it shows: the loop is {listed}",
                    chain[at].name
                );
                self.report(&owner, show.line, &target, Code::ShownLoop, message);
                continue;
            }
            if chain.len() >= MAX_CHAIN_DEPTH {
                let message = format!(
                    "`{}` would be the 17th composition of its chain, and a chain holds at most {MAX_CHAIN_DEPTH}, the root counted",
                    show.src
                );
                self.report(&owner, show.line, &target, Code::ShownTooDeep, message);
                continue;
            }
            match self.reach(&target, &show.src) {
                Reached::Missing => {
                    let message = format!("the composition `{}` shown here is not in the project folder", show.src);
                    self.report(&owner, show.line, &target, Code::ShownMissing, message);
                }
                Reached::Unreadable(reason) => {
                    let message = format!("the composition `{}` shown here could not be read: {reason}", show.src);
                    self.report(&owner, show.line, &target, Code::ShownUnreadable, message);
                }
                Reached::Found {
                    name,
                    shows: inner,
                    problems,
                } => {
                    if let (Some(outer), Some(theirs)) = (shows.fps, inner.fps) {
                        if outer != theirs {
                            let message = format!(
                                "`{}` has {theirs} frames a second and the composition that shows it has {outer}; they must be the same",
                                show.src
                            );
                            self.report(&owner, show.line, &target, Code::ShownFpsDiffers, message);
                        }
                    }
                    for problem in problems {
                        self.reports.push(Report::new(show.src.clone(), problem));
                    }
                    if !inner.elements.is_empty() && self.walked.insert((target.clone(), chain.len() + 1)) {
                        chain.push(Link { path: target, name });
                        self.walk(chain, &inner);
                        chain.pop();
                    }
                }
            }
        }
    }

    /// Reads the file at `path` the first time an element names it, judging it and keeping it when it
    /// is valid; later reaches give what was read then.
    fn reach(&mut self, path: &Path, src: &str) -> Reached {
        if let Some((name, shows)) = self.cache.get(path) {
            return Reached::Found {
                name: name.clone(),
                shows: shows.clone(),
                problems: Vec::new(),
            };
        }
        let source = match fs::read(path) {
            Ok(source) => source,
            Err(error) if error.kind() == ErrorKind::NotFound => return Reached::Missing,
            Err(error) => return Reached::Unreadable(reason(&error)),
        };
        let shows = notation::shows(&source);
        let problems = match notation::read(&source) {
            Ok(composition) => {
                self.files.push(ShownFile {
                    name: src.to_string(),
                    path: path.to_path_buf(),
                    root: composition.root,
                    tree: composition.tree,
                });
                Vec::new()
            }
            Err(problems) => problems,
        };
        self.cache.insert(path.to_path_buf(), (src.to_string(), shows.clone()));
        Reached::Found {
            name: src.to_string(),
            shows,
            problems,
        }
    }

    /// Reports a problem of the element at `line` in the file `owner`, unless that report was made.
    fn report(&mut self, owner: &Link, line: u32, target: &Path, code: Code, message: String) {
        if self.seen.insert((owner.path.clone(), line, code, target.to_path_buf())) {
            self.reports
                .push(Report::new(owner.name.clone(), Problem::new(code, line, message)));
        }
    }
}
