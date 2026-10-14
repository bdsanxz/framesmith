# The record of the meanings of the codes

Status: in force from Sprint 1. This is the record that `error-catalogue.md` section 5 describes: for each code that has been in a version of the list, the meaning it had when it was recorded. A row is never edited and never removed; a code whose problem can no longer happen keeps its row. A test holds the list in `error-catalogue.md` to this record (section 6 there): a meaning that differs from its row, or a recorded code that is no longer on the list, fails it and names the code.

A code with no row here is new. Its row is added when the code is first merged to the main line.

| Code | Meaning when recorded |
|---|---|
| `FS-0001` | The composition file is not valid UTF-8 text. |
| `FS-0002` | The composition file is not well-formed XML. |
| `FS-0003` | The root element of the composition file is not `<composition>`. |
| `FS-0004` | The root `<composition>` has no `version`. |
| `FS-0005` | The root's `version` is not one that this program reads. |
| `FS-0006` | The root has no `width` or no `height`. |
| `FS-0007` | The root's `width` or `height` is not an even whole number from 2 to 8192. |
| `FS-0008` | The root's `fps` is not a whole number from 1 to 240. |
| `FS-0009` | The root has no `duration`. |
| `FS-0010` | The root's `duration` is not a time in seconds. |
| `FS-0011` | The root's `duration` is 0. |
| `FS-0012` | The root's `background` is not one colour. |
| `FS-0013` | The folder given to the command does not exist. |
| `FS-0014` | The path given to the command is not a folder. |
| `FS-0015` | The folder given to the command holds no `composition.xml`. |
| `FS-0016` | The `composition.xml` of the folder exists and could not be read. |
| `FS-0017` | The command line cannot be read. |
| `FS-0018` | The project file is not valid UTF-8 text. |
| `FS-0019` | The project file is not well-formed YAML. |
| `FS-0020` | The `framesmith.yaml` of the folder exists and could not be read. |
| `FS-0021` | The project file is not one mapping of keys. |
| `FS-0022` | A key of the project file is not one that the project file has. |
| `FS-0023` | A key of the project file is known and is not supported in this version. |
| `FS-0024` | A value in the project file is of the wrong kind or outside the values its key takes. |
| `FS-0025` | The composition named by the project file does not exist. |
| `FS-0026` | The composition named by the project file exists and could not be read. |
| `FS-0027` | The path given to the command does not exist. |
| `FS-0028` | The path given to the command exists and could not be read. |
