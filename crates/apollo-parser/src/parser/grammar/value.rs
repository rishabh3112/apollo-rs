use crate::{
    parser::grammar::{name, variable},
    Parser, SyntaxKind, TokenKind, S, T,
};

#[derive(Clone, Copy)]
pub(crate) enum Constness {
    Const,
    NotConst,
}

/// See: https://spec.graphql.org/October2021/#Value
///
/// *Value[Const]*
///     [if not Const] Variable
///     IntValue
///     FloatValue
///     StringValue
///     BooleanValue
///     NullValue
///     EnumValue
///     ListValue[?Const]
///     ObjectValue[?Const]
pub(crate) fn value(p: &mut Parser, constness: Constness, pop_on_error: bool) {
    match p.peek() {
        Some(T![$]) => {
            if let Constness::Const = constness {
                let error_message = "unexpected variable value in a Const context";
                if pop_on_error {
                    p.err_and_pop(error_message);
                } else {
                    p.err(error_message);
                }
            }
            // Consume the variable name even if const, for better error recovery
            variable::variable(p);
        }
        Some(TokenKind::Int) => {
            let _g = p.start_node(SyntaxKind::INT_VALUE);
            p.bump(SyntaxKind::INT);
        }
        Some(TokenKind::Float) => {
            let _g = p.start_node(SyntaxKind::FLOAT_VALUE);
            p.bump(SyntaxKind::FLOAT);
        }
        Some(TokenKind::StringValue) => {
            let _g = p.start_node(SyntaxKind::STRING_VALUE);
            p.bump(SyntaxKind::STRING);
        }
        Some(TokenKind::Name) => {
            let node = p.peek_data().unwrap();
            match node.as_str() {
                "true" => {
                    let _g = p.start_node(SyntaxKind::BOOLEAN_VALUE);
                    p.bump(SyntaxKind::true_KW);
                }
                "false" => {
                    let _g = p.start_node(SyntaxKind::BOOLEAN_VALUE);
                    p.bump(SyntaxKind::false_KW);
                }
                "null" => {
                    let _g = p.start_node(SyntaxKind::NULL_VALUE);
                    p.bump(SyntaxKind::null_KW)
                }
                _ => enum_value(p),
            }
        }
        Some(T!['[']) => list_value(p, constness),
        Some(T!['{']) => object_value(p, constness),
        _ => {
            let error_message = "expected a valid Value";
            if pop_on_error {
                p.err_and_pop(error_message);
            } else {
                p.err(error_message);
            }
        }
    }
}
/// See: https://spec.graphql.org/October2021/#EnumValue
///
/// *EnumValue*:
///     Name *but not* **true** *or* **false** *or* **null**
pub(crate) fn enum_value(p: &mut Parser) {
    let _g = p.start_node(SyntaxKind::ENUM_VALUE);
    let name = p.peek_data().unwrap();

    if matches!(name.as_str(), "true" | "false" | "null") {
        p.err("unexpected Enum Value");
    }

    name::name(p);
}

/// See: https://spec.graphql.org/October2021/#ListValue
///
/// *ListValue[Const]*:
///     **[** **]**
///     **[** Value[?Const]* **]**
pub(crate) fn list_value(p: &mut Parser, constness: Constness) {
    let _g = p.start_node(SyntaxKind::LIST_VALUE);
    p.bump(S!['[']);

    while let Some(node) = p.peek() {
        if node == T![']'] {
            p.bump(S![']']);
            break;
        } else if node == TokenKind::Eof {
            break;
        } else {
            if p.recursion_limit.check_and_increment() {
                p.limit_err("parser recursion limit reached");
                return;
            }
            value(p, constness, true);
            p.recursion_limit.decrement()
        }
    }
}

/// See: https://spec.graphql.org/October2021/#ObjectValue
///
/// *ObjectValue[Const]*:
///     **{** **}**
///     **{** ObjectField[?Const]* **}**
pub(crate) fn object_value(p: &mut Parser, constness: Constness) {
    let _g = p.start_node(SyntaxKind::OBJECT_VALUE);
    p.bump(S!['{']);

    while let Some(TokenKind::Name) = p.peek() {
        object_field(p, constness);
    }

    p.expect(T!['}'], S!['}']);
}

/// See: https://spec.graphql.org/October2021/#ObjectField
///
/// *ObjectField[Const]*:
///     Name **:** Value[?Const]
pub(crate) fn object_field(p: &mut Parser, constness: Constness) {
    let _guard = p.start_node(SyntaxKind::OBJECT_FIELD);
    name::name(p);

    if let Some(T![:]) = p.peek() {
        p.bump(S![:]);
        if p.recursion_limit.check_and_increment() {
            p.limit_err("parser recursion limit reached");
            return;
        }
        value(p, constness, true);
        p.recursion_limit.decrement()
    }
}

/// See: https://spec.graphql.org/October2021/#DefaultValue
///
/// *DefaultValue*:
///     **=** Value[Const]
pub(crate) fn default_value(p: &mut Parser) {
    let _g = p.start_node(SyntaxKind::DEFAULT_VALUE);
    p.bump(S![=]);
    value(p, Constness::Const, false);
}

#[cfg(test)]
mod test {
    use crate::{cst, cst::CstNode, Parser};

    #[test]
    fn it_returns_string_for_string_value_into() {
        let schema = r#"
enum Test @dir__one(string: "string value", int_value: -10, float_value: -1.123e+4, bool: false) {
  INVENTORY
} "#;
        let parser = Parser::new(schema);
        let cst = parser.parse();

        assert!(cst.errors.is_empty());

        let document = cst.document();
        for definition in document.definitions() {
            if let cst::Definition::EnumTypeDefinition(enum_) = definition {
                for directive in enum_.directives().unwrap().directives() {
                    for argument in directive.arguments().unwrap().arguments() {
                        if let cst::Value::StringValue(val) =
                            argument.value().expect("Cannot get argument value.")
                        {
                            let source = val.source_string();
                            assert_eq!(source, r#""string value""#);

                            let contents: String = val.into();
                            assert_eq!(contents, "string value");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn it_unescapes_strings() {
        let schema = r#"
"String with\tescapes\r\n"
scalar StringWithEscapes
"String with unicode \uadf8\ub77c\ud504\ud050\uc5d8"
scalar StringWithUnicode
"""
Escapes\nshould\nnot\nmatter
including \q nonexistent \W ones
\""" is the only one \""
"""
scalar BlockStringRaw
"#;
        let parser = Parser::new(schema);
        let cst = parser.parse();

        assert!(cst.errors.is_empty());

        let mut expected = vec![
            "String with\tescapes\r\n",
            "String with unicode 그라프큐엘",
            r#"
Escapes\nshould\nnot\nmatter
including \q nonexistent \W ones
""" is the only one \""
"#
            .trim(),
        ]
        .into_iter();

        let document = cst.document();
        for definition in document.definitions() {
            let cst::Definition::ScalarTypeDefinition(scalar) = definition else {
                continue;
            };
            let description = scalar.description().unwrap().string_value().unwrap();
            let s = String::from(description);
            assert_eq!(s, expected.next().unwrap());
        }
    }

    #[test]
    fn it_returns_i64_for_int_values() {
        let schema = r#"
enum Test @dir__one(int_value: -10) {
  INVENTORY
} "#;
        let parser = Parser::new(schema);
        let cst = parser.parse();

        assert!(cst.errors.is_empty());

        let document = cst.document();
        for definition in document.definitions() {
            if let cst::Definition::EnumTypeDefinition(enum_) = definition {
                for directive in enum_.directives().unwrap().directives() {
                    for argument in directive.arguments().unwrap().arguments() {
                        if let cst::Value::IntValue(val) =
                            argument.value().expect("Cannot get argument value.")
                        {
                            let i: i32 = val.try_into().unwrap();
                            assert_eq!(i, -10);
                        }
                    }
                }
            }
        }
    }

    #[test]
    // Allow only for this test, as this tests doesn't actually aim to compare
    // floats, but is here to ensure we are able to extract an f64 value
    #[allow(clippy::float_cmp)]
    fn it_returns_f64_for_float_values() {
        let schema = r#"
enum Test @dir__one(float_value: -1.123E4) {
  INVENTORY
} "#;
        let parser = Parser::new(schema);
        let cst = parser.parse();

        assert!(cst.errors.is_empty());

        let document = cst.document();
        for definition in document.definitions() {
            if let cst::Definition::EnumTypeDefinition(enum_) = definition {
                for directive in enum_.directives().unwrap().directives() {
                    for argument in directive.arguments().unwrap().arguments() {
                        if let cst::Value::FloatValue(val) =
                            argument.value().expect("Cannot get argument value.")
                        {
                            let f: f64 = val.try_into().unwrap();
                            assert_eq!(f, -1.123E4);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn it_returns_bool_for_boolean_values() {
        let schema = r#"
enum Test @dir__one(bool_value: false) {
  INVENTORY
} "#;
        let parser = Parser::new(schema);
        let cst = parser.parse();

        assert!(cst.errors.is_empty());

        let document = cst.document();
        for definition in document.definitions() {
            if let cst::Definition::EnumTypeDefinition(enum_) = definition {
                for directive in enum_.directives().unwrap().directives() {
                    for argument in directive.arguments().unwrap().arguments() {
                        if let cst::Value::BooleanValue(val) =
                            argument.value().expect("Cannot get argument value.")
                        {
                            let b: bool = val.try_into().unwrap();
                            assert!(!b);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn it_parses_variable_names() {
        let input = "
query GraphQuery($graph_id: ID!, $variant: String) {
  service(id: $graph_id) {
    schema(tag: $variant) {
      document
    }
  }
}
        ";
        let parser = Parser::new(input);
        let cst = parser.parse();
        assert_eq!(0, cst.errors().len());

        let doc = cst.document();

        for def in doc.definitions() {
            if let cst::Definition::OperationDefinition(op_def) = def {
                assert_eq!(op_def.name().unwrap().text(), "GraphQuery");

                let variable_defs = op_def.variable_definitions();
                let variables: Vec<String> = variable_defs
                    .iter()
                    .flat_map(|v| v.variable_definitions())
                    .filter_map(|v| Some(v.variable()?.text().to_string()))
                    .collect();
                assert_eq!(
                    variables.as_slice(),
                    ["graph_id".to_string(), "variant".to_string()]
                );
            }
        }
    }

    #[test]
    fn it_parse_mutation_with_escaped_char() {
        let input = r#"mutation {
            createStore(draft: {
              name: [{ locale: "en", value: "\"my store\"" }]
            }) {
              name(locale: "en")
            }
          }"#;
        let parser = Parser::new(input);
        let cst = parser.parse();

        assert!(cst.errors.is_empty());
    }

    #[test]
    fn it_parse_mutation_without_escaped_char() {
        let input = r#"mutation {
            createStore(draft: {
              name: [{ locale: "en", value: "my store" }]
            }) {
              name(locale: "en")
            }
          }"#;
        let parser = Parser::new(input);
        let cst = parser.parse();

        assert!(cst.errors.is_empty());
    }

    #[test]
    fn it_parse_mutation_without_escaped_char_with_error() {
        let input = r#"mutation {
            createStore(draft: {
              name: [{ locale: "en", value: "\"my store" }]
            }) {
              name(locale: "en")
            }
          }"#;
        let parser = Parser::new(input);
        let cst = parser.parse();

        assert!(cst.errors.is_empty());
    }

    #[test]
    fn it_parse_mutation_with_escaped_chars_and_without() {
        let input = r#"mutation {
            createStore(draft: {
              name: [{ locale: "en", value: "my \a store" }]
            }) {
              name(locale: "en")
            }
          }"#;
        let parser = Parser::new(input);
        let cst = parser.parse();

        assert!(!cst.errors.is_empty());
    }

    #[test]
    fn it_returns_error_for_unfinished_string_value_in_list() {
        let schema = r#"extend schema
  @link(url: "https://specs.apollo.dev/federation/v2.0",
        import: ["@key", "@external])
        
type Vehicle @key(fields: "id") {
  id: ID!,
  type: String,
  modelCode: String,
  brandName: String,
  launchDate: String
}
"#;

        let parser = Parser::new(schema);
        let cst = parser.parse();
        assert!(!cst.errors.is_empty());
    }
}
