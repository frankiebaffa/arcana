## Installation

The _arcana compiler_ can be downloaded with `git` and installed with `cargo`.

```bash
# download a copy of the repository
git clone "https://github.com/frankiebaffa/arcana"
cd arcana
# install the arcc compiler
cargo install --path compiler
arcc -h
```

See the [help](https://raw.githubusercontent.com/frankiebaffa/arcana/master/compiler/resources/help.txt)
document for the usage of the compiler.

## Glossary

**alias:** A reference to a value within the current _context_ (i.e.
`value.is.here`).

**chain:** A hyphen (`-`) character following the closure of tags such as
if, for, and their respective else. Tells the parser to ignore whitespace until
the next block opening.

**content:** Characters included in the output of the parser.

**context:** A map of values.

**pathlike:** A literal path (i.e. `"path/to/file.txt"`) or an _alias_ to a
path in the current _context_ (i.e. `context.path.to.file`).

**sealed:** A context scope whose modifications will not propagate to a higher
level.

**stringlike:** A string, number, or boolean in the current _context_.

## Whitespace Control

```arcana
This is an example \
    of whitespace control.
```

Arcana has one mode of whitespace control, a single backslash ending a line.
The parser will ignore the backslash and consume all following whitespace
without dumping to _content_. The above example would compile to the following.

```arcana
This is an example of whitespace control.
```

## Tags

Expression tags can control the flow of the document, spawn other parsers, and
can write content to other files.

### Comment

```arcana
#{This is a comment.}#
```

Comments are ignored by the parser. Comments are closed with a special endblock
so they can span multiple lines and contain the templating syntax without
prematurely closing.

```arcana
#{ Uncomment in production
${alias}
}#
```

### Extend-Template

```arcana
+{pathlike}
```

A template to parse using the final context of the current file. The output
_content_ of the current file will be set to the special _alias_ `$content`.

### Source-File

```arcana
.{pathlike}
```

A _context_ file to include in the current _context_. Matching values will be
overwritten.

#### Modifiers

##### As

```arcana
.{pathlike|as obj}
```

The `as` modifier can be used to specify an _alias_ at which to place the values
sourced from the specified _context_ file.

Consider the _context_ file `context.json`:

```json
{
    "name": "Jane Doe",
    "age": 42
}
```

And the _template_ file `template.arcana`:

```arcana
.{"context.json"|as person}
${person.name}: ${person.age}
```

When compiled, the _template_ file would yield the result:

```txt
Jane Doe: 42
```

### Include-File

```arcana
&{pathlike}
```

A file to parse and include in the position of the tag. Any changes to the
_context_ while parsing the file are _sealed_.

An optional block can be included which will allow for modifications to the
_sealed context_ prior to parsing the given file. Any output of this block will
also be included in the _sealed context_ with the special `$content` _alias_.

Consider the following file `link.arcana`.

```arcana
<a href="${href}"%{cls}( class="${cls}")">${$content}</a>
```

It can be included in another template like so:

```arcana
<p>Click &{"./link.arcana"}(\
    ={}({
        "href": "https://github.com",
        "cls": "a-link"
    })\
    Here\
)</p>
```

Or without the JSON literal block:

```arcana
<p>Click &{"./link.arcana"}(\
    ={href}("https://github.com")\
    ={cls}("a-link")\
    Here\
)</p>
```

Either of these templates would compile to the following:

```arcana
<p>Click <a href="https://github.com" class="a-link">Here</a></p>
```

#### Modifiers

##### Raw

```arcana
&{pathlike|raw}
```

Does not parse the content, only includes directly. Can be used in conjunction
with the _markdown_ modifier.

##### Markdown

```arcana
&{pathlike|md}
```

Parses the output of the include as [No-Flavor Markdown](http://frankiebaffa.com/software/nfm.html).
Can be used in conjunction with the _raw_ modifier.

### If

```arcana
%{alias}-
(${alias})-
(Alias is falsey.)
```

Evaluates the output of the condition as true or false. If true, the first
trailing block is parsed. Else, the second trailing block is parsed. The default
condition of the _if_ tag is _truthy_. The initial _if_ tag as well as the
_true_ condition tag can be optionally trailed by a _chain_ as shown above. The
condition can be preceeded by the _not_ operator (`!`) to negate the evaluated
condition.

#### Exists

```arcana
%{alias exists}(${alias})(Alias does not exist.)
```

Evaluates to true if the given _alias_ exists in the current _context_.

#### Empty

```arcana
%{!alias empty}{${alias}}{Alias was empty.}
```

Evaluates to true if the given _alias_ has an empty value in the current
_context_.

#### Truthy

```arcana
%{this.alias}(Alias was true.)(Alias was false.)
```

Evaluates to true if the given _alias_ is truthy. A defined string will always
be true. A number will be true when greater than 0. An array will always be
true. An object will be evaluated based on whether or not it contains any keys.
Null will always evaluate to false.

#### Comparisons

```arcana
%{this.alias==that.alias}()
%{this.alias>that.alias}()
%{this.alias>=that.alias}()
%{this.alias<that.alias}()
%{this.alias<=that.alias}()
```

Evaluates the equality or comparison between two JSON objects. If the objects
cannot be compared (string to number, etc), then an error will be thrown.

#### Multiple Conditions

```arcana
%{this.alias&&this.alias>that.alias||$loop exists}()
```

Conditions can be chained together using `&&` for `and` and `||` for `or`.

### For-Each-Item

```arcana
@{item in alias}(
    ${item.name}
)(
    No items.
)
```

Loop through contents of an array in _context_. The inner _context_ of the loop
is _sealed_. The first trailing block will be parsed for every item found within
the array. If there are no items, the second block will be parsed. The
_for-each-item_ tag can be trailed by a _chain_.

#### Loop Context

For loops initialize a special _alias_ into the _sealed context_ named `$loop`.
This object's values are mapped to the following aliases.

**$loop.index:** The 0-indexed position of the current iteration.

**$loop.position:** The 1-indexed position of the current iteration.

**$loop.length:** The length of the array being iterated over.

**$loop.max:** The maximum index of the array begin iterated over.

**$loop.first:** Set iff the current value of `$loop.index` is 0.

**$loop.last:** Set iff the current value of `$loop.index` is `$loop.max`.

#### Modifiers

##### Paths

Treat the values found within the array as paths to files.

```arcana
@{dir in alias|paths}(
    *{file in dir|ext "json"}(
        .{file|as subobj}
        ${subobj.description}
    )(
        No files in directory.
    )
)(
    No directories in alias.
)
```

##### Reverse

Reverse the order of the array.

```arcana
@{item in alias|reverse}-
(%{ !$loop.first }(
)${ item })-
(No items.)
```

### For-Each-File

```arcana
*{file in pathlike}(
    &{item}
)(
    No files in directory.
)
```

Loop through the files found within a given directory. The inner context of the
loop is sealed. The trailing blocks function identically to the _for-each-item_
tag.

#### Loop Context

The same loop context is set for this tag as for _for-each-item_.

#### Modifiers

##### With-Extension

```arcana
*{file in "./this/dir"|ext "json"}(
    .{file|as sub}
    *{sub-file in sub.files}(
        \(${$loop.position}\) ${sub-file|filename}
    )
)
```

Only include files with the matching extension.

##### Reverse

```arcana
*{file in pathlike|ext "arcana"|reverse}(
    &{file}
)(
    No files found in directory.
)
```

Reverse the order of the files.

### Include-Content

```arcana
${alias.to.stringlike}
```

Includes the _stringlike_ value of the _alias_ from the current _context_ in the
_content_.

#### Modifiers

##### Lower

```arcana
${alias|lower}
```

Changes the _content_ to lowercase.

##### Replace

```arcana
${alias|replace "x" "y"}
```

Replaces instances of `x` with `y`.

##### Upper

```arcana
${alias|upper}
```

Changes the _content_ to uppercase.

##### Path

```arcana
${ alias | path }
```

Handles the _content_ as a path.

##### Filename

```arcana
${ alias | path | filename }
```

Outputs only the filename of the path.

##### Trim

```arcana
${alias|trim}
```

Removes whitespace from the start and end of the _content_.

##### Split

```arcana
${alias|split 2 0}
```

Splits the _content_ into `2` parts and uses the part at index `0`.

Consider the _context_ file `context.json`:

```json
{
    "name": "Jane Doe"
}
```

And the _template_ file `template.arcana`:

```arcana
.{"context.json"}
${name|split 2 1}
```

The output of the parser would be:

```txt
 Doe
```

##### Json

```arcana
={item}("This string here.")
={ctx.item}(${item|json})
```

Represents the content as raw json.

### Set-Item

```arcana
={alias}("Here is the value")
```

Sets the value at _alias_ within the current _context_ to the parsed JSON output
of the block.

The block can contain arcana syntax as long as the parsed output is valid JSON.

```arcana
={this-name}(${item.name|json})
```

Any JSON type can be set using the _set-item_ block.

```arcana
={this-object}({
    "key": "value",
    "items": [
        "first",
        "second",
        "third"
    ],
    "a-number": 54.2
})
```

The root context can also be written-to by avoiding the inclusion of an alias.

```arcana
={}({
    "root-level-item": "Some value."
})
${root-level-item}
```

This comes in handy when an included file references an object from the root
context and is accessed from within a loop or another context. Consider the
following file `artist.arcana`:

```arcana
# ${name}

${brief}

@{album in albums}(
    &{"./album.arcana"}{
        ={}(${album|json})
    }
)
```

The file considers the `artist` object to be at the root level. So if we wanted
to include the file from another context, say `artists.arcana`, we could
reference the artist as a root level object:

```arcana
@{artist in artists}(
    &{"./artist.arcana"|md}(
        ={}(${artist|json})
    )
)
```

### Unset Item

```arcana
/{alias}
```

Unsets the value at _alias_.

## File Operation Tags

The following tags perform file operations and are intended for deployment
purposes. If you're feeling a bit _nutty_, you can use them in your standard
templates however you wish.

### Write-Content

```arcana
^{pathlike}(\
    &{"this/file.arcana"}(={title}("A title here"))\
)
```

Writes to _pathlike_ the _content_ of the block.

### Copy-Path

```arcana
~{source-pathlike destination-pathlike}
```

Copies the file at the source _pathlike_ to the file at the destination
_pathlike_.

### Delete-Path

```arcana
-{pathlike}
```

Deletes the file at _pathlike_.

### Example

Using the file operation tags, you can write your own logging deployment files
like the following.

```arcana
#{ ./deploy.arcana }#\
={font-file}("./in/font.ttf")\
={font-dest}("./out/fonts/font.ttf")\
Copying font "${font-file|path}" to "${font-dest|path}"\
~{font-file font-dest}
Compiling templates...\
*{template in "./in/templates"|files|ext "arcana"}(
    ={template-dest}("./out/${$loop.entry.stem}.html")\
    Compiling "${template|path}" to "${template-dest|path}"\
    ^{template-dest}(&{template})\
)\
```

Running the `arcc ./deploy.arcana` command would output the following while also
properly deploying the files.

```txt
Compiling font "./in/font.ttf" to "./out/fonts/font.ttf"
Compiling templates...
    Compiling "./in/templates/first.arcana" to "./out/first.html"
    Compiling "./in/templates/second.arcana" to "./out/second.html"
```
