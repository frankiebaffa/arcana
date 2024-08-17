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

## Tags

Expression tags control the flow of the document and typically conform to the
following pattern:

```txt
<EXP>{\s*<PATHORALIAS>\s*[|<MOD>[\s*<MODARGS>]*]*\s*}
```

### Comment

```arcana
#{This is a comment.}#
```

Comments are ignored by the parser. If a comment is followed by a trailing
line-break, it will also be ignored. Comments are closed with a special endblock
so they can span multiple lines and contain the templating syntax without
prematurely closing.

```arcana
#{ Uncomment in production
${alias}
}#
```

### Ignore

```arcana
!{This file is ignored.}!
```

Disregards anything following the tag. The inner content of this block is
ignored, but is useful for placing the reason why the file is ignored. Ignores
are closed with a special endblock so they can span multiple lines and contain
the templating syntax without prematurely closing.

```arcana
!{ File is not yet ready.
@{file in files}{
    &{file.path}{
        ={$root}<{file.content}
    }
}
}!
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

```arcana
&{pathlike}{
    ={alias-1}{This is a property.}
    ={alias-2}{This is another.}
}
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
{${alias}}-
{Alias does not exist.}
```

Evaluates the output of the condition as true or false. If true, the first
trailing block is parsed. Else, the second trailing block is parsed. The default
condition of the _if_ tag is _exists_. The initial _if_ tag as well as the
_true_ condition tag can be optionally trailed by a _chain_ as shown above. The
condition can be preceeded by the _not_ operator (`!`) to negate the evaluated
condition.

#### Exists

```arcana
%{alias exists}{${alias}}{Alias does not exist.}
```

Evaluates to true if the given _alias_ exists in the current _context_.

#### Empty

```arcana
%{!alias empty}{${alias}}{Alias was empty.}
```

Evaluates to true if the given _alias_ has an empty value in the current
_context_.

### For-Each-Item

```arcana
@{item in alias}{
    ${item.name}
}{
    No items.
}
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
@{dir in alias|paths}{
    *{file in dir|ext "json"}{
        .{file|as subobj}
        ${subobj.description}
    }{
        No files in directory.
    }
}{
    No directories in alias.
}
```

##### Reverse

Reverse the order of the array.

```arcana
@{item in alias|reverse}-
{%{ !$loop.first }{
}${ item }}-
{No items.}
```

### For-Each-File

```arcana
*{file in pathlike}{
    &{item}
}{
    No files in directory.
}
```

Loop through the files found within a given directory. The inner context of the
loop is sealed. The trailing blocks function identically to the _for-each-item_
tag.

#### Loop Context

The same loop context is set for this tag as for _for-each-item_.

#### Modifiers

##### With-Extension

```arcana
*{file in pathlike|ext "arcana"}{
    &{file}{
        ={$loop}<{loop}
    }
}
```

Only include files with the matching extension.

##### Reverse

```arcana
*{file in pathlike|ext "arcana"|reverse}{
    &{file}
}{
    No files found in directory.
}
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

### Set-Item

```arcana
={alias}{Here is the value}
```

Sets the value at _alias_ within the current _context_ to the _content_ of the
block. This can also be used with other items in the _context_.

```arcana
={alias}{${item.name}}
```

#### Modifiers

##### Array

Initializes an array at _alias_ if it is not an already an array and pushes the
given value into it.

```arcana
={alias|array}{First item}
```

This modifier can also be chained together using chains or inline.

```arcana
={alias|array}{First item}{Second item}-
{Third item}{Fourth item}
```

##### Path

Sets _alias_ to a pathlike built from the output of the block.

```arcana
={alias|path}{path/to/file.txt}
```

#### Siphon-Item

```arcana
={alias1}<{alias2}
```

Sets _alias1_ to the literal json value found at _alias2_. This is useful when
an entire object needs to be set at a different alias while including another
template. The special `$root` alias can be used in the first position iff
_alias2_ is a map and all of the keys are wished to be moved to the root of the
current _context_.

The following example utilizes the _siphon-item_ tag as well as the `$root`
alias. If the file `album.arcana` is used both from another template as well as
within a standalone template, it may only reference the properties within
`album` at their root alias, i.e. `name`, instead of accessing them from a
higher scoped alias such as `album.name`.

```arcana
@{album in artist.albums}{
    &{"album.arcana"}{
        ={$root}<{album}
    }
}
```

### Unset Item

```arcana
/{alias}
```

Unsets the value at _alias_.

#### Modifiers

##### Pop

Pops the last value off of the array.

```arcana
/{alias|pop}
```

