# Search Operators

The search box finds phrases by trigger, name, description and text. In [[Advanced Mode]] you can narrow the search with operators.

| Operator | Searches |
|---|---|
| `trigger:` | the triggers |
| `label:` | the phrase name |
| `folder:` | the folder name |
| `file:` | the file path |
| `comment:` | the description |
| `body:` | the text |

* A word on its own searches triggers, name, description and text.
* Use quotes for a phrase with spaces: `body:"kind regards"`.
* Several terms must **all** match (AND): `folder:work trigger:;sig`.
* Search ignores capitals and finds the text anywhere in the field.
* Something like `:hi` or `http:x` is treated as an ordinary word, because it is not an operator.
* With Advanced off the whole box is one plain search.

Search terms you add to a phrase (under **More options**) are also found by plain search.
