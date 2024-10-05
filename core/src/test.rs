//! Tests for the Arcana Templating Engine.
// Copyright (C) 2024  Frankie Baffa
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use {
    crate::{
        context::JsonContext,
        error::Error,
        file::Source,
        parser::Parser,
    },
    serde_json::{
        from_str as from_json_str,
        json,
        Value as JsonValue,
    },
    std::{
        env::current_dir,
        path::PathBuf,
    },
};

#[test]
fn json_test() {
    let json = r#"
        {
            "name": "Somebody",
            "age": 31
        }
    "#;

    let obj = from_json_str::<JsonValue>(json).unwrap();

    assert_eq!(&json!("Somebody"), &obj["name"]);
    assert_eq!(&json!(31), &obj["age"]);
    assert_eq!(&JsonValue::Null, &obj["not-a-value"]);
}

#[test]
fn json_context_1() {
    let mut curr = current_dir().unwrap();

    let mut a_path = curr.clone();

    let path = "test/json_context/1/ctx.json";
    curr.push(path);
    a_path.push("test/json_context/1/path/to/file.txt");

    let ctx = JsonContext::read(curr).unwrap();
    let path_prop = ctx.get_path("path").unwrap();
    assert_eq!(a_path, path_prop);
}

#[test]
#[should_panic]
fn json_context_1_err() {
    JsonContext::read("test/json_context/1/ctx.json").unwrap();
}

#[test]
fn json_context_2() {
    let mut first_ctx = current_dir().unwrap();
    let mut second_ctx = first_ctx.clone();

    let mut ctx_1_file_1 = first_ctx.clone();
    let mut ctx_1_file_2 = first_ctx.clone();
    let mut ctx_2_file_1 = first_ctx.clone();
    let mut ctx_2_file_2 = first_ctx.clone();

    first_ctx.push("test/json_context/2/ctx_1.json");
    second_ctx.push("test/json_context/2/sub/ctx_2.json");

    ctx_1_file_1.push("test/json_context/2/path/to/first.file");
    ctx_1_file_2.push("test/json_context/2/path/to/second.file");
    ctx_2_file_1.push("test/json_context/2/sub/path/to/first.file");
    ctx_2_file_2.push("test/json_context/2/path/to/third.file");

    let mut ctx = JsonContext::read(first_ctx).unwrap();

    let path_prop = ctx.get_path("path").unwrap();
    assert_eq!(ctx_1_file_1, path_prop);

    // should overrite property "path"
    ctx.read_in(second_ctx).unwrap();

    let path_prop = ctx.get_path("path").unwrap();
    assert_eq!(ctx_2_file_1, path_prop);

    let other_path_prop = ctx.get_path("other_path").unwrap();
    assert_eq!(ctx_1_file_2, other_path_prop);

    let the_other_path_prop = ctx.get_path("the_other_path").unwrap();
    assert_eq!(ctx_2_file_2, the_other_path_prop);
}

#[test]
fn json_context_3() {
    let mut ctx = JsonContext::faux_context("/file.txt").unwrap();
    let mut map = JsonValue::Object(serde_json::Map::new());
    map.as_object_mut().unwrap().insert("first".to_owned(), JsonValue::String("value".to_owned()));
    map.as_object_mut().unwrap().insert("second".to_owned(), JsonValue::String("value".to_owned()));
    for (k, v) in map.as_object().unwrap().into_iter() {
        ctx.set_value(k, "/file.txt".into(), v.to_owned()).unwrap();
    }

    assert_eq!("value", ctx.get_stringlike("first").unwrap());
    assert_eq!("value", ctx.get_stringlike("second").unwrap());
}

#[test]
fn source_struct_1() {
    let mut source = Source::read_file("test/source/1/source.txt").unwrap();
    let against = "First line\nsecond line";

    for c in against.chars() {
        assert_eq!(c.to_string(), source.take(1).unwrap());
    }

    assert!(source.eof());
}

#[test]
fn escape_1() {
    let mut p = Parser::new("test/escape/1/escape.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "\\section*{The name.}",
        p.as_output()
    );
}

#[test]
fn escape_2() {
    let mut p = Parser::new("test/escape/2/esc.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "\\\nA test.",
        p.as_output()
    );
}

#[test]
fn escape_3() {
    let mut p = Parser::new("test/escape/3/esc.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        concat!(
            "This is a test with some escaped breaks inside of it. And a ",
            "literal \\ character. And \\\nthat.",
        ),
        p.as_output()
    );
}

#[test]
fn escape_4() {
    let mut p = Parser::new("test/escape/4/esc.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "This has a literal backslash followed by a trim right \\there.",
        p.as_output()
    );
}

#[test]
#[should_panic]
fn escape_5() {
    let mut p = Parser::new("test/escape/5/esc.arcana").unwrap();
    p.parse().unwrap();
}

#[test]
fn parser_1() {
    let mut cf = current_dir().unwrap();
    cf.push("test/parser/1/test.txt");
    let mut cd = cf.clone();
    cd.pop();

    let mut p = Parser::new("test/parser/1/test.txt").unwrap();

    assert_eq!(cd, p.directory());
    assert_eq!(&cf, p.file());

    let src_line_1 = "This is a test file.\n";
    assert_eq!(src_line_1, p.src().pos());

    p.src_mut().take(src_line_1.len()).unwrap();

    let src_line_2 = "With some text in it.";
    assert_eq!(src_line_2, p.src().pos());
}

#[test]
fn parser_2() {
    let mut p = Parser::new("test/parser/2/test.txt").unwrap();
    p.read_ctx_in("ctx.json").unwrap();
    let ctx = p.ctx().as_ref().unwrap();
    let name = ctx.get("name").unwrap();
    assert_eq!("A name", name.as_str().unwrap());
}

#[test]
fn comment_1() {
    let mut p = Parser::new("test/comment/1/comment.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("And this is some text.", p.as_output());
}

#[test]
fn comment_2() {
    let mut p = Parser::new("test/comment/2/comment.txt").unwrap();
    match p.parse() {
        Ok(_) => panic!("Test should have panicked!"),
        Err(e) => match e {
            Error::UnterminatedTag(name, c, _) => {
                assert_eq!("comment", name);
                assert_eq!(c.line(), 0);
                assert_eq!(c.position(), 0);
            },
            _ => panic!("Error should have been CommentTagNotEnded"),
        },
    }
}

#[test]
fn comment_3() {
    let mut p = Parser::new("test/comment/3/comment.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("And this is some text here.", p.as_output());
}

#[test]
fn comment_4() {
    let mut p = Parser::new("test/comment/4/comment.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("", p.as_output());
}

#[test]
fn extends_1() {
    let mut p = Parser::new("test/extends/1/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "This is second.",
        p.as_output()
    );
}

#[test]
fn extends_2() {
    let mut p = Parser::new("test/extends/2/file.txt").unwrap();
    if let Err(Error::ContextEmpty(c, _)) = p.parse() {
        assert_eq!(0, c.line());
        assert_eq!(11, c.position());
    }
    else {
        panic!("Should have returned ContextEmpty error.");
    }
}

#[test]
fn extends_3() {
    let mut p = Parser::new("test/extends/3/file.txt").unwrap();
    if let Err(Error::UnterminatedPath(c, _)) = p.parse() {
        assert_eq!(0, c.line());
        assert_eq!(3, c.position());
    }
    else {
        panic!("Should have returned UnterminatedPath error.");
    }
}

#[test]
fn extends_4() {
    let mut p = Parser::new("test/extends/4/file.txt").unwrap();
    if let Err(Error::UnterminatedTag(name, c, _)) = p.parse() {
        assert_eq!("extends", name);
        assert_eq!(0, c.line());
        assert_eq!(0, c.position());
    }
    else {
        panic!("Should have returned UnterminatedTag error.");
    }
}

#[test]
fn extends_5() {
    let mut p = Parser::new("test/extends/5/page.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("Should be the Name", p.as_output());
}

#[test]
fn extends_5_content() {
    let mut p = Parser::new("test/extends/5/page_content.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("Should be the Name\nThis should be saved as $content.", p.as_output());
}

#[test]
fn source_tag_1() {
    let mut p = Parser::new("test/source_tag/1/source.txt").unwrap();
    p.parse().unwrap();
    let name = p.enforce_context(|ctx| ctx.get_stringlike("name")).unwrap();
    assert_eq!("The Name", name);
    let desc = p.enforce_context(|ctx| ctx.get_stringlike("desc")).unwrap();
    assert_eq!("Here is a brief description.", desc);
    let long_description = p.enforce_context(|ctx| ctx.get_stringlike("full_description"))
        .unwrap();
    assert_eq!(
        "Here is a full description for the thing. It is a bit clunkier.",
        long_description,
    );
}

#[test]
fn include_content_1() {
    let mut p = Parser::new("test/include_content/1/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("The name", p.as_output());
}

#[test]
fn include_content_2() {
    let mut p = Parser::new("test/include_content/2/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("A NAME", p.as_output());
}

#[test]
fn include_content_3() {
    let mut p = Parser::new("test/include_content/3/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("HXrX is a namX", p.as_output());
}

#[test]
fn include_content_4() {
    let mut p = Parser::new("test/include_content/4/file.txt").unwrap();

    let res = p.parse();
    if let Err(Error::ValueNotString(a)) = res {
        assert_eq!("name", a.to_string());
    }
    else if let Err(e) = res {
        panic!("{e}");
    }
    else {
        panic!("Supposed to return ValueNotString error, was Ok");
    }
}

#[test]
fn include_content_5() {
    let mut p = Parser::new("test/include_content/5/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("escription.", p.as_output());
}

#[test]
fn include_content_6() {
    let mut p = Parser::new("test/include_content/6/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("Let's s\nplit in\nto three.", p.as_output());
}

#[test]
fn include_content_7() {
    let mut p = Parser::new("test/include_content/7/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("Let's\n spli\nt int\no three.", p.as_output());
}

#[test]
fn include_content_8() {
    let mut p = Parser::new("test/include_content/8/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("Split\nThis", p.as_output());
}

#[test]
fn include_content_9() {
    let mut p = Parser::new("test/include_content/9/file.arcana").unwrap();
    p.parse().unwrap();
    let against = include_str!("../test/include_content/9/against.txt");
    assert_eq!(against[0..against.len()-1], p.as_output());
}

#[test]
fn include_file_1() {
    let mut p = Parser::new("test/include_file/1/file1.txt").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "This is the first file's description.\nThis is the first file's description.",
        p.as_output()
    );
}

#[test]
fn include_file_2() {
    let mut p = Parser::new("test/include_file/2/file.txt").unwrap();
    p.parse().unwrap();
    assert_eq!(
        concat!(
            "!{ This is how you ignore a file }\n",
            ".{ \"here/is/a/context.json\" }\n",
            "${ this.is.a.variable }"
        ),
        p.as_output()
    );
}

#[test]
fn include_file_3() {
    let mut p = Parser::new("test/include_file/3/test.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "This is the internal value.\nThis is the content.\n\n",
        p.as_output()
    );
}

#[test]
fn include_file_4() {
    let mut p = Parser::new("test/include_file/4/include-file.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "This file is included.",
        p.as_output()
    );
}

#[test]
fn if_tag_1_if() {
    let mut p = Parser::new("test/if_tag/1/if.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("The name", p.as_output());
}

#[test]
fn if_tag_1_else() {
    let mut p = Parser::new("test/if_tag/1/else.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("No description.", p.as_output());
}

#[test]
fn if_tag_2_if() {
    let mut p = Parser::new("test/if_tag/2/if.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("A Name\n31", p.as_output());
}

#[test]
fn if_tag_3() {
    let mut p = Parser::new("test/if_tag/3/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("\neq\neq\nle\nge\nge\nle\ntrue\neq\neq\nle\nge\nge\nle\ntrue", p.as_output());
}

#[test]
fn if_tag_4() {
    let mut p = Parser::new("test/if_tag/4/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("\nne\nne\nle\nlt\nlt\nle\ntrue\nne\nne\ngt\nge\nge\ngt\ntrue", p.as_output());
}

#[test]
fn if_tag_5() {
    let mut p = Parser::new("test/if_tag/5/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "\nne\nne\ngt\nge\nge\ngt\ntrue\nnot-empty\nexists\nne\nne\nle\nlt\nlt\nle\nfalse\nnot-empty\nexists",
        p.as_output()
    );
}

#[test]
fn if_tag_6() {
    let mut p = Parser::new("test/if_tag/6/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "\ntrue\nfalse\ntrue\ntrue",
        p.as_output()
    );
}

#[test]
fn if_tag_7() {
    let mut p = Parser::new("test/if_tag/7/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "False",
        p.as_output()
    );
}

#[test]
fn if_tag_8() {
    let mut p = Parser::new("test/if_tag/8/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        "True",
        p.as_output()
    );
}

#[test]
fn if_tag_9() {
    let mut p = Parser::new("test/if_tag/9/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This.", p.as_output());
}

#[test]
fn if_tag_10() {
    let mut p = Parser::new("test/if_tag/10/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is not a loop.", p.as_output());
}

#[test]
fn if_tag_11() {
    let mut p = Parser::new("test/if_tag/11/if.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is not a loop.", p.as_output());
}

#[test]
fn for_file_1() {
    let mut p = Parser::new("test/for_file/1/for.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("First item.\nSecond item.\nThird item.\nEnd.", p.as_output());
}

#[test]
fn for_file_2() {
    let mut p = Parser::new("test/for_file/2/for.txt").unwrap();
    p.parse().unwrap();
    assert_eq!(
        concat!(
            "Items 1 First.: Items 2 First.\nItems 1 First.: Items 2 Second.\n",
            "Items 1 Second.: Items 2 First.\nItems 1 Second.: Items 2 Second.\n",
            "End."
        ),
        p.as_output(),
    );
}

#[test]
fn for_file_3() {
    let mut p = Parser::new("test/for_file/3/for-file.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First, Second, Third.", p.as_output());
}

#[test]
fn for_file_4() {
    let mut p = Parser::new("test/for_file/4/for-file.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("test.txt", p.as_output());
}

#[test]
fn for_file_5() {
    let mut p = Parser::new("test/for_file/5/for-file.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("sub1, sub2, sub3", p.as_output());
}

#[test]
fn for_item_1() {
    let mut p = Parser::new("test/for_item/1/for.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("First\nSecond\nThird\nEnd.", p.as_output());
}

#[test]
fn for_item_2() {
    let mut p = Parser::new("test/for_item/2/for.txt").unwrap();
    p.parse().unwrap();
    assert_eq!("First\nSecond\nThird\nEnd.", p.as_output());
}

#[test]
fn for_item_3() {
    let mut p1 = Parser::new("test/for_item/3/one.arcana").unwrap();
    p1.parse().unwrap();
    assert_eq!("First, Second", p1.as_output());

    let mut p2 = Parser::new("test/for_item/3/two.arcana").unwrap();
    p2.parse().unwrap();
    assert_eq!("", p2.as_output());
}

#[test]
fn for_item_4() {
    let mut p = Parser::new("test/for_item/4/for-item.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First, Second, Third", p.as_output());
}

#[test]
fn full_1() {
    let mut p = Parser::new("test/full/1/page.html").unwrap();
    p.parse().unwrap();
    assert_eq!(
        concat!(
            "<!DOCTYPE html>\n",
            "<html>\n",
            "\t<head>\n",
            "\t\t<title>Full Test 1</title>\n",
            "\t</head>\n",
            "\t<body>\n",
            "\t\t<ul>\n",
            "\t\t\t<li>First.</li>\n",
            "\t\t\t<li>Second.</li>\n",
            "\t\t</ul>\n",
            "\t</body>\n",
            "</html>",
        ),
        p.as_output(),
    );
}

#[test]
fn full_2() {
    let mut p = Parser::new("test/full/2/base.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        concat!(
            "<div class=\"toc\">\n",
            "\t<p>Parent</p>\n",
            "\t\n",
            "\t\t\n",
            "\t\t<div class=\"toc\">\n",
            "\t<a href=\"https://duckduckgo.com\">First</a>\n",
            "\t\n",
            "</div><div class=\"toc\">\n",
            "\t<a href=\"https://start.duckduckgo.com\">Second</a>\n",
            "\t\n",
            "</div>\n",
            "\t\n",
            "</div>",
        ),
        p.as_output()
    );
}

#[test]
fn set_item_1() {
    let mut p = Parser::new("test/set_item/1/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is the item description.", p.as_output());
}

#[test]
fn set_item_2() {
    let mut p = Parser::new("test/set_item/2/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First\nSecond\nThird\nFourth", p.as_output());
}

#[test]
fn set_item_3() {
    let mut p = Parser::new("test/set_item/3/set.arcana").unwrap();
    p.parse().unwrap();
    let mut current = current_dir().unwrap();
    current.push("test/set_item/3/path/to/file.txt");
    assert_eq!(
        current.to_str().unwrap_or("").to_owned(),
        p.as_output()
    );
}

#[test]
fn set_item_4() {
    let mut p = Parser::new("test/set_item/4/set.arcana").unwrap();
    p.parse().unwrap();
    let current = current_dir().unwrap();
    let mut first = current.clone();
    first.push("test/set_item/4/path/to/file.txt");
    let first_str = first.to_str().unwrap_or("");
    let mut second = current.clone();
    second.push("test/different/file.txt");
    let second_str = second.to_str().unwrap_or("");
    let third = PathBuf::from("/absolute/path.txt");
    let third_str = third.to_str().unwrap_or("");
    assert_eq!(
        format!("{first_str}\n{second_str}\n{third_str}"),
        p.as_output()
    );
}

#[test]
fn set_item_5() {
    let mut p = Parser::new("test/set_item/5/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is the value\nThis is the value", p.as_output());
}

#[test]
fn set_item_6() {
    let mut p = Parser::new("test/set_item/6/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is the value\nThis is the value", p.as_output());
}

#[test]
fn set_item_7() {
    let mut p = Parser::new("test/set_item/7/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is the value\nThis is the value", p.as_output());
}

#[test]
fn set_item_8_has_item() {
    let mut p = Parser::new("test/set_item/8/has-item.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("\n\t\tHere is the name.\n", p.as_output());
}

#[test]
fn set_item_8_no_item() {
    let mut p = Parser::new("test/set_item/8/no-item.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("\n\tNo name.\n", p.as_output());
}

#[test]
fn set_json_1() {
    let mut p = Parser::new("test/set_json/1/template.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First: 31", p.as_output());
}

#[test]
fn set_json_2() {
    let mut p = Parser::new("test/set_json/2/template.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First", p.as_output());
}

#[test]
#[should_panic]
fn set_json_3() {
    let mut p = Parser::new("test/set_json/3/template.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First", p.as_output());
}

#[test]
fn set_json_4() {
    let mut p = Parser::new("test/set_json/4/template.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First, Second, Third", p.as_output());
}

#[test]
fn set_json_5() {
    let mut p = Parser::new("test/set_json/5/template.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("The Name", p.as_output());
}

#[test]
fn set_json_6() {
    let mut p = Parser::new("test/set_json/6/template.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("{\"name\":\"An Object\"}", p.as_output());
}

#[test]
fn set_json_7() {
    let mut p = Parser::new("test/set_json/7/template.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!(
        std::fs::canonicalize("./test/set_json/7/template.arcana").unwrap(),
        PathBuf::from(p.as_output())
    );
}

#[test]
fn set_json_8() {
    let mut p = Parser::new("test/set_json/8/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is the item description.", p.as_output());
}

#[test]
fn set_json_9() {
    let mut p = Parser::new("test/set_json/9/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First\nSecond\nThird\nFourth", p.as_output());
}

#[test]
fn set_json_10() {
    let mut p = Parser::new("test/set_json/10/set.arcana").unwrap();
    p.parse().unwrap();
    let mut current = current_dir().unwrap();
    current.push("test/set_json/10/path/to/file.txt");
    assert_eq!(
        current.to_str().unwrap_or("").to_owned(),
        p.as_output()
    );
}

#[test]
fn set_json_11() {
    let mut p = Parser::new("test/set_json/11/set.arcana").unwrap();
    p.parse().unwrap();
    let current = current_dir().unwrap();
    let mut first = current.clone();
    first.push("test/set_json/11/path/to/file.txt");
    let first_str = first.to_str().unwrap_or("");
    let mut second = current.clone();
    second.push("test/different/file.txt");
    let second_str = second.to_str().unwrap_or("");
    let third = PathBuf::from("/absolute/path.txt");
    let third_str = third.to_str().unwrap_or("");
    assert_eq!(
        format!("{first_str}\n{second_str}\n{third_str}"),
        p.as_output()
    );
}

#[test]
fn set_json_12() {
    let mut p = Parser::new("test/set_json/12/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is the value\nThis is the value", p.as_output());
}

#[test]
fn set_json_13() {
    let mut p = Parser::new("test/set_json/13/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is the value\nThis is the value", p.as_output());
}

#[test]
fn set_json_14() {
    let mut p = Parser::new("test/set_json/14/set.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("This is the value\nThis is the value", p.as_output());
}

#[test]
fn set_json_15_has_item() {
    let mut p = Parser::new("test/set_json/15/has-item.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("\n\tHere is the name.\n", p.as_output());
}

#[test]
fn set_json_15_no_item() {
    let mut p = Parser::new("test/set_json/15/no-item.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("\n\tNo name.\n", p.as_output());
}

#[test]
fn unset_item_1() {
    let mut p = Parser::new("test/unset_item/1/unset.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("", p.as_output());
}

#[test]
#[should_panic]
fn unset_item_2() {
    let mut p = Parser::new("test/unset_item/2/unset.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("First", p.as_output());
}

#[test]
fn unset_item_3() {
    let mut p = Parser::new("test/unset_item/3/unset.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("", p.as_output());
}

#[test]
fn unset_item_4() {
    let mut p = Parser::new("test/unset_item/4/unset.arcana").unwrap();
    p.parse().unwrap();
    assert_eq!("", p.as_output());
}

#[test]
fn delete_path_1() {
    let path: PathBuf = "test/delete_path/1/delete-this.txt".into();
    std::fs::write(&path, &[0x00]).unwrap();
    assert!(path.exists());

    let mut p = Parser::new("test/delete_path/1/delete.arcana").unwrap();
    p.parse().unwrap();

    assert!(!path.exists());
}

#[test]
fn write_content_1() {
    let path: PathBuf = "test/write_content/1/write-this.txt".into();
    assert!(!path.exists());

    let mut p = Parser::new("test/write_content/1/write-content.arcana").unwrap();
    p.parse().unwrap();

    assert!(path.exists());

    assert_eq!(
        "some content here",
        std::fs::read_to_string(&path).unwrap().lines().collect::<Vec<&str>>().join("\n"),
    );

    std::fs::remove_file(path).unwrap();

    assert_eq!("", p.as_output());
}

#[test]
fn write_content_2() {
    let path: PathBuf = "test/write_content/2/new-file.txt".into();
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }

    let mut p = Parser::new("test/write_content/2/write.arcana").unwrap();
    p.parse().unwrap();

    assert!(path.exists());

    assert_eq!(
        "NAME: Write Content 2\nDATE: 2024-09-17",
        std::fs::read_to_string(&path).unwrap().lines().collect::<Vec<&str>>().join("\n"),
    );

    std::fs::remove_file(path).unwrap();

    assert_eq!("", p.as_output());
}

#[test]
fn copy_path_1() {
    let path: PathBuf = "test/copy_path/1/to-here.txt".into();
    assert!(!path.exists());

    let mut p = Parser::new("test/copy_path/1/copy.arcana").unwrap();
    p.parse().unwrap();

    assert!(path.exists());

    assert_eq!(
        "This should be the content.",
        std::fs::read_to_string(&path).unwrap().lines().collect::<Vec<&str>>().join("\n"),
    );

    std::fs::remove_file(path).unwrap();

    assert_eq!("", p.as_output());
}
