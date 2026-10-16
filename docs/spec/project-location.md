# Naming a project: a folder, a project file or a composition file

Status: in force from Sprint 1, for every command that takes a project (`framesmith check` now; `render` and `info` take it the same way when they are built). Sources: `ROADMAP.md` line 1.1 (a folder holds a project) and line 1.10 (the project file); `notation.md` section 3 (a project is a folder; every `src` is a path relative to the project folder, wherever the XML file lies); `project-file.md` sections 2, 3.1 and 4 (the project file, the kind "path", the composition it names); `check-command.md` and `error-catalogue.md` (the line a refusal prints, the codes); `run-log.md` (what is logged); decisions P2 of `DECISIONS.md` (the exit codes). Where this file and `notation.md` say the same thing, `notation.md` is the text and this file only points to it.

Authors and scripts name a project in different ways. One rule covers all three, so that the same project is read whichever way it is named and a path written in it means the same thing.

## 1. The three forms of the argument

The argument is one path. What is at the path decides the form (section 2); nothing in its spelling does, except a closing `/` (section 5).

| The path is | The form | The project folder is | The composition read is |
|---|---|---|---|
| A folder | Folder | The folder itself | The one its `framesmith.yaml` names with `composition`, or `composition.xml` when it has no project file or the file names none |
| A file named `framesmith.yaml` | Project file | The folder the file is in | The same as for the folder |
| Any other file | Composition file | The nearest folder at or above the file that holds a `framesmith.yaml`; when there is none, the folder the file is in (section 3) | The file itself, whatever `composition` says |

- **A folder is never searched upward.** It is the project folder, with its `framesmith.yaml` if it has one. `check demo/scenes` reads `demo/scenes` as a project, whether or not `demo` has a project file.
- **A project file makes its folder the project folder.** Nothing above it matters.
- **A composition file is the one rendered.** When the project file names another composition with `composition`, the argument wins: the key is still judged as a value (a wrong value is reported with its line), and the file it names is not read and need not exist. A command line that gives a composition file means that file.
- **The project file is read in every form** when the project folder has one, and its reports come before those of the composition, as for a folder (`project-file.md` section 4).
- **A file of any other name is a composition file.** The command does not look at its extension: a file that is not a well-formed composition is reported by its own code (`FS-0001`, `FS-0002`, `FS-0003`).

## 2. What is at the path

The path is examined once, following links:

- Nothing there: the path does not exist (section 5).
- A folder: the form Folder.
- A file with the name `framesmith.yaml`: the form Project file. The name is compared letter for letter.
- A file with another name: the form Composition file.
- Something that is neither a folder nor a file (a named pipe, a device): it is not read, and it is reported as not a folder (section 5). A pipe would wait for ever for a writer.
- A path that cannot be examined for a reason of the machine (a folder on the way that this user may not enter): reported as unreadable, a failure (section 5).

## 3. Finding the project file for a composition file

1. Take the folder the file is in, made absolute and followed to the real folder (links resolved, `.` and `..` removed). A file given by a bare name (`intro.xml`) is in the folder the command is run from.
2. Look in that folder and then in each folder above it, one at a time, up to the root of the disk, for a `framesmith.yaml`. The first folder that holds an entry of that name is the project folder. An entry that is not a readable file (a folder named `framesmith.yaml`) is found like any other, and reported when it is read (`FS-0020`), as for the form Folder.
3. When no folder holds one, the file's own folder is the project folder, and the project has no project file: every key has its default.

The search is a rule about the folders above the file and about nothing else: it reads no file but the project file it finds, and it stops at the first. It is not limited to a repository or to the user's home folder: a `framesmith.yaml` anywhere above the file is the project's.

The project folder is written as the argument gave it when it is the folder the file is in (`demo/scenes` for `check demo/scenes/intro.xml` when `demo/scenes` holds the project file), and as the absolute real path when it is a folder above (`/Users/ana/demo`).

## 4. The base of every path

- **Every path written in the project is relative to the project folder** and written with `/`: every `src` of a composition (`notation.md` section 3), `composition`, `paths.renders` and `render.output` of the project file (`project-file.md` section 3). It is never relative to the folder the command is run from, and never to the folder of the composition file that holds it: `src="logo.png"` in `scenes/intro.xml` is `logo.png` of the project folder, not `scenes/logo.png`.
- **One function makes the path.** `Project::resolve` takes a path as it is written and gives the path of the file: the project folder, then the written path. A written path that begins with `/` is a path of the machine and is taken as it is (`project-file.md` section 3.1). On every system it is returned exactly as it was written, and no part of the project folder is joined to it. That is not left to the system's join of paths, which on Windows keeps the drive of a folder that has one. The `composition` a project file names is made the same way when `Project::open` reads it. Whether a `src` may be one is judged where media is checked, not here.
- **The name of a file in a report** is its path from the project folder, written with `/` (`check-command.md` section 4): `scenes/intro.xml` for a composition in a sub-folder, `intro.xml` for one that is in the project folder.
- `Project::composition` gives the composition that is read, as a path from the project folder; `Project::composition_path` gives the path of the file.

## 5. What is reported

A path that does not exist, and a folder with nothing to render, are `No project` lines (`check-command.md` section 5): the outcome, the code, the kind, the reason and the remedy, on one line, with the exit code 1. Each reason names the path as it was written on the command line, and nothing else of the machine.

| Code | Kind | When | The reason |
|---|---|---|---|
| `FS-0027` | invalid | Nothing is at the path, written without a closing `/` | `` `demo/intro.xml` does not exist `` |
| `FS-0013` | invalid | Nothing is at the path, written with a closing `/` (a path written so names a folder) | ``the folder `demo/` does not exist`` |
| `FS-0014` | invalid | A closing `/` is written and the path is a file, or the path is neither a folder nor a file | `` `demo/intro.xml/` is not a folder `` |
| `FS-0015` | invalid | The project folder holds no `composition.xml`, and its project file (if it has one) names no other composition | ``there is no composition.xml in `demo`, and no framesmith.yaml there names a composition`` |
| `FS-0028` | failure | The path exists and could not be examined or read (a composition file this user may not read) | `` `demo/intro.xml` could not be read: `` and the reason the system gives |

The reason of `FS-0015` names the folder and both files that were looked for: `composition.xml`, and a `framesmith.yaml` that names a composition. The codes `FS-0016`, `FS-0020`, `FS-0025` and `FS-0026` keep their meanings and their lines (`check-command.md` section 5, `project-file.md` section 4). A path is never reported as missing when it exists, and a file is never read as a folder.

A path written with a closing `/` can only be a folder; that is why a missing one is the older `FS-0013`, and why a file written so is `FS-0014`. The path of a file is written without one.

## 6. It changes nothing

Naming a project in any of the three forms reads the same files as the folder form (the project file, and the one composition) and changes, renames and deletes none (`check-command.md` section 6). The search of section 3 only looks whether `framesmith.yaml` exists in each folder above the file.

## 7. What is logged

The run log gets one `debug` line when the argument has been examined and the project folder found: `project located`, with the form (`folder`, `project file` or `composition file`) and `folders_searched`, the number of folders looked in for a project file (0 for the first two forms; for a composition file, 1 when the file's own folder holds it, and one more for each folder above it that was looked in). It holds no path (`run-log.md` section 6). The outcomes of the run are those of `run-log.md` section 4, with the new codes among those of `no project`.

## 8. Where the code lives

- `notation` holds the two new codes (`Code::PathMissing`, `Code::PathUnreadable`).
- `project` examines the path and finds the project folder (`locate`), opens the project (`Project::open`, which takes a path of any of the three forms), and makes the path of a written file (`Project::resolve`).
- `cli` passes the argument as it was written and prints.
