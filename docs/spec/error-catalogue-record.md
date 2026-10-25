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
| `FS-0029` | An element of the composition file is not one that the notation has. |
| `FS-0030` | An element of the composition file has an attribute that the element does not take. |
| `FS-0031` | An element of the composition file stands in a place where the notation does not allow it. |
| `FS-0032` | A value in the composition file that must be a number is not one. |
| `FS-0033` | A value in the composition file that must be a time in seconds is not one. |
| `FS-0034` | A length in the composition file is not a number of pixels or a percentage, or is below 0 where 0 or more is taken. |
| `FS-0035` | The project uses a form that is known and that this version does not draw or apply. |
| `FS-0036` | The video could not be made. |
| `FS-0037` | An option of `render`, or its value, is not one the command takes. |
| `FS-0038` | The author asked for the graphics card (`gpu`), and this machine has no card that frx7 can draw with. |
| `FS-0039` | A value that must be one of a list of words is not one of them. |
| `FS-0040` | An element of the composition file has `start` without `duration`, or `duration` without `start`, where the notation writes both or neither. |
| `FS-0041` | A `<style>` of the composition file has no `id`, so no element can take it. |
| `FS-0042` | Two `<style>` elements of the composition file have the same `id`. |
| `FS-0043` | An element's `class` names a style that the composition file does not have. |
| `FS-0044` | A composition shows itself, directly or through the compositions it shows. |
| `FS-0045` | A chain of compositions shown by `<composition src>` is more than 16 compositions deep, the root counted. |
| `FS-0046` | A composition shown by `<composition src>` has another frame rate from the composition that shows it. |
| `FS-0047` | A composition shown by `<composition src>` does not exist in the project folder. |
| `FS-0048` | A composition shown by `<composition src>` is in the project folder and could not be read. |
| `FS-0049` | A font file that `<font src>` names does not exist, at the path it gives from the project folder or, when it begins with `/`, on the machine. |
| `FS-0050` | A font file that `<font src>` names exists and could not be read. |
| `FS-0051` | A file that `<font src>` names is not a font of a kind this version reads, which are TrueType (`.ttf`) and font collections (`.ttc`). |
| `FS-0052` | A text attribute's value is not in the form the notation gives for it. |
| `FS-0053` | A length or size that must be above 0 is 0 or below. |
| `FS-0054` | A `columns` of a `<box>` is not a list of column widths: each a number of pixels above 0, or a share such as `1fr`. |
| `FS-0055` | A `<box>` has both `columns` and `flex-direction`. |
| `FS-0056` | A character of a text whose picture this version does not draw: the font has no PNG picture for it at any size, or frx7 refuses the picture. |
| `FS-0057` | A file that `<font src>` names begins as a font of a kind this version reads, and the font reader refuses it as damaged. |
| `FS-0058` | A `<tween>` has no `duration`, or a `duration` of 0. |
| `FS-0059` | A `<tween>` has no `from-` or `to-` attribute. |
| `FS-0060` | A `<tween>` changes a property that the element it moves does not take as an attribute. |
