# Editing YAML

Your phrases live in YAML files. In [[Advanced Mode]] you can edit them directly, with the same safety net as the form.

## Edit one phrase

1. Turn on **Advanced** and select a phrase.
2. Open the **YAML** tab.
3. Press **Edit YAML**.
4. Change the text. Tab adds two spaces, Shift+Tab removes them, Ctrl+S saves, Esc leaves.
5. Press **Save**.

![The YAML editor](images/yaml-editor.png)

The comment block above the phrase is part of the text you edit.

## Rules

* The text must be **exactly one** phrase.
* It is checked as you type ("Checking...", then "Valid" or an error with line and column). **Save** is disabled while it is invalid.
* Every other phrase in the file must stay unchanged, or the save is refused.
* If the file changed on disk since you opened it, xpando tells you and offers **Reload** instead of overwriting.
* Saving is atomic and keeps a `.bak` backup, as for form edits.

## Edit the whole file

Turn on the **Whole file** switch to edit every phrase and top-level key (such as `imports` or `global_vars`). A phrase that disappears must be confirmed. Switching between one phrase and the whole file asks before discarding what you changed ("Discard your changes?" with **Discard** or **Keep editing**).

## Tips

* Comments and layout in your files are kept when xpando saves from the form.
* New to the file format? See the [espanso documentation](https://espanso.org/docs/matches/basics/).
