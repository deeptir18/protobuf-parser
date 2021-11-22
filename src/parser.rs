use std::str;
use std::ops::Range;

use super::{EnumValue, Enumeration, Extension, Field, FieldType, FileDescriptor, Message, OneOf,
    Rule, Syntax};
use nom::{digit, hex_digit, multispace};

fn is_word(b: u8) -> bool {
    match b {
        b'a'...b'z' | b'A'...b'Z' | b'0'...b'9' | b'_' | b'.' => true,
        _ => false,
    }
}

named!(
    word<String>,
    map_res!(take_while!(is_word), |b: &[u8]| String::from_utf8(
        b.to_vec()
    ))
);
named!(
    word_ref<&str>,
    map_res!(take_while!(is_word), str::from_utf8)
);

named!(
    hex_integer<i32>,
    do_parse!(
        tag!("0x") >> num: map_res!(map_res!(hex_digit, str::from_utf8), |s| {
            i32::from_str_radix(s, 16)
        }) >> (num)
    )
);

named!(
    integer<i32>,
    map_res!(map_res!(digit, str::from_utf8), str::FromStr::from_str)
);

named!(
    comment<()>,
    do_parse!(tag!("//") >> take_until_and_consume!("\n") >> ())
);
named!(
    block_comment<()>,
    do_parse!(tag!("/*") >> take_until_and_consume!("*/") >> ())
);

/// word break: multispace or comment
named!(
    br<()>,
    alt!(map!(multispace, |_| ()) | comment | block_comment)
);

named!(
    syntax<Syntax>,
    do_parse!(
        tag!("syntax") >> many0!(br) >> tag!("=") >> many0!(br)
            >> proto:
                alt!(tag!("\"proto2\"") => { |_| Syntax::Proto2 } |
                             tag!("\"proto3\"") => { |_| Syntax::Proto3 }) >> many0!(br)
            >> tag!(";") >> (proto)
    )
);

named!(
    import<String>,
    do_parse!(
        tag!("import") >> many1!(br) >> tag!("\"")
            >> path: map_res!(take_until!("\""), |b: &[u8]| String::from_utf8(b.to_vec()))
            >> tag!("\"") >> many0!(br) >> tag!(";") >> (path)
    )
);

named!(
    package<String>,
    do_parse!(
        tag!("package") >> many1!(br) >> package: word >> many0!(br) >> tag!(";") >> (package)
    )
);

named!(
    num_range<Range<i32>>,
    do_parse!(
        from_: integer >> many1!(br) >> tag!("to") >> many1!(br) >> to_: integer
            >> (from_..to_.saturating_add(1))
    )
);

named!(
    reserved_nums<Vec<Range<i32>>>,
    do_parse!(
        tag!("reserved") >> many1!(br)
            >> nums:
                separated_list!(
                    do_parse!(many0!(br) >> tag!(",") >> many0!(br) >> (())),
                    alt!(num_range | integer => { |i: i32| i..i.saturating_add(1) })
                ) >> many0!(br) >> tag!(";") >> (nums)
    )
);

named!(
    reserved_names<Vec<String>>,
    do_parse!(
        tag!("reserved") >> many1!(br)
            >> names:
                many1!(do_parse!(
                    tag!("\"") >> name: word >> tag!("\"")
                        >> many0!(alt!(br | tag!(",") => { |_| () })) >> (name)
                )) >> many0!(br) >> tag!(";") >> (names)
    )
);

named!(
    key_val<(&str, &str)>,
    do_parse!(
        tag!("[") >> many0!(br) >> key: word_ref >> many0!(br) >> tag!("=") >> many0!(br)
            >> value: map_res!(is_not!("]"), str::from_utf8) >> tag!("]") >> many0!(br)
            >> ((key, value.trim()))
    )
);

named!(
    rule<Rule>,
    alt!(tag!("optional") => { |_| Rule::Optional } |
            tag!("repeated") => { |_| Rule::Repeated } |
            tag!("required") => { |_| Rule::Required } )
);

named!(
    field_type<FieldType>,
    alt!(tag!("int32") => { |_| FieldType::Int32 } |
            tag!("int64") => { |_| FieldType::Int64 } |
            tag!("uint32") => { |_| FieldType::Uint32 } |
            tag!("uint64") => { |_| FieldType::Uint64 } |
            tag!("sint32") => { |_| FieldType::Sint32 } |
            tag!("sint64") => { |_| FieldType::Sint64 } |
            tag!("fixed32") => { |_| FieldType::Fixed32 } |
            tag!("sfixed32") => { |_| FieldType::Sfixed32 } |
            tag!("fixed64") => { |_| FieldType::Fixed64 } |
            tag!("sfixed64") => { |_| FieldType::Sfixed64 } |
            tag!("bool") => { |_| FieldType::Bool } |
            tag!("string") => { |_| FieldType::String } |
            tag!("ref_counted_string") => { |_| FieldType::RefCountedString } |
            tag!("bytes") => { |_| FieldType::Bytes } |
            tag!("ref_counted_bytes") => { |_| FieldType::RefCountedBytes } |
            tag!("float") => { |_| FieldType::Float } |
            tag!("double") => { |_| FieldType::Double } |
            tag!("group") => { |_| FieldType::Group(Vec::new()) } |
            map_field => { |(k, v)| FieldType::Map(Box::new((k, v))) } |
            word => { |w| FieldType::MessageOrEnum(w) })
);

named!(
    map_field<(FieldType, FieldType)>,
    do_parse!(
        tag!("map") >> many0!(br) >> tag!("<") >> many0!(br) >> key: field_type >> many0!(br)
            >> tag!(",") >> many0!(br) >> value: field_type >> tag!(">") >> ((key, value))
    )
);

named!(
    fields_in_braces<Vec<Field>>,
    do_parse!(
        tag!("{") >> many0!(br)
        >> fields: separated_list!(br, message_field)
        >> many0!(br) >> tag!("}") >> (fields)
    )
);

named!(
    one_of<OneOf>,
    do_parse!(
        tag!("oneof") >> many1!(br) >> name: word >> many0!(br)
            >> fields: fields_in_braces >> many0!(br)
            >> (OneOf {
                name: name,
                fields: fields,
            })
    )
);

named!(
    group_fields_or_semicolon<Option<Vec<Field>>>,
    alt!(
        tag!(";") => { |_| None } |
        fields_in_braces => { Some })
);

named!(
    message_field<Field>,
    do_parse!(
        rule: opt!(rule) >> many0!(br) >> typ: field_type >> many1!(br) >> name: word >> many0!(br)
            >> tag!("=") >> many0!(br) >> number: integer >> many0!(br)
            >> key_vals: many0!(key_val) >> many0!(br)
            >> group_fields: group_fields_or_semicolon >> ({

                let typ = match (typ, group_fields) {
                    (FieldType::Group(..), Some(group_fields)) => {
                        FieldType::Group(group_fields)
                    }
                    // TODO: produce error if semicolon is after group or group is without fields
                    (typ, _) => typ
                };

                Field {
                    name: name,
                    rule: rule.unwrap_or(Rule::Optional),
                    typ: typ,
                    number: number,
                    default: key_vals
                        .iter()
                        .find(|&&(k, _)| k == "default")
                        .map(|&(_, v)| v.to_string()),
                    packed: key_vals
                        .iter()
                        .find(|&&(k, _)| k == "packed")
                        .map(|&(_, v)| str::FromStr::from_str(v).expect("Cannot parse Packed value")),
                    deprecated: key_vals
                        .iter()
                        .find(|&&(k, _)| k == "deprecated")
                        .map_or(false, |&(_, v)| str::FromStr::from_str(v)
                            .expect("Cannot parse Deprecated value")),
                }})
    )
);

enum MessageEvent {
    Message(Message),
    Enumeration(Enumeration),
    Field(Field),
    ReservedNums(Vec<Range<i32>>),
    ReservedNames(Vec<String>),
    OneOf(OneOf),
    Ignore,
}

named!(
    message_event<MessageEvent>,
    alt!(reserved_nums => { |r| MessageEvent::ReservedNums(r) } |
                                         reserved_names => { |r| MessageEvent::ReservedNames(r) } |
                                         message_field => { |f| MessageEvent::Field(f) } |
                                         message => { |m| MessageEvent::Message(m) } |
                                         enumerator => { |e| MessageEvent::Enumeration(e) } |
                                         one_of => { |o| MessageEvent::OneOf(o) } |
                                         br => { |_| MessageEvent::Ignore })
);

named!(
    message_events<(String, Vec<MessageEvent>)>,
    do_parse!(
        tag!("message") >> many1!(br) >> name: word >> many0!(br) >> tag!("{") >> many0!(br)
            >> events: many0!(message_event) >> many0!(br) >> tag!("}") >> many0!(br)
            >> many0!(tag!(";")) >> ((name, events))
    )
);

named!(
    message<Message>,
    map!(
        message_events,
        |(name, events): (String, Vec<MessageEvent>)| {
            let mut msg = Message {
                name: name.clone(),
                ..Message::default()
            };
            for e in events {
                match e {
                    MessageEvent::Field(f) => msg.fields.push(f),
                    MessageEvent::ReservedNums(r) => msg.reserved_nums = r,
                    MessageEvent::ReservedNames(r) => msg.reserved_names = r,
                    MessageEvent::Message(m) => msg.messages.push(m),
                    MessageEvent::Enumeration(e) => msg.enums.push(e),
                    MessageEvent::OneOf(o) => msg.oneofs.push(o),
                    MessageEvent::Ignore => (),
                }
            }
            msg
        }
    )
);

named!(
    extensions<Vec<Extension>>,
    do_parse!(
        tag!("extend") >> many1!(br) >> extendee: word >> many0!(br) >>
            fields: fields_in_braces >> (
                fields.into_iter().map(|field| Extension {
                    extendee: extendee.clone(),
                    field
                }).collect()
            )
    )
);

named!(
    enum_value<EnumValue>,
    do_parse!(
        name: word >> many0!(br) >> tag!("=") >> many0!(br) >> number: alt!(hex_integer | integer)
            >> many0!(br) >> tag!(";") >> many0!(br) >> (EnumValue {
            name: name,
            number: number,
        })
    )
);

named!(
    enumerator<Enumeration>,
    do_parse!(
        tag!("enum") >> many1!(br) >> name: word >> many0!(br) >> tag!("{") >> many0!(br)
            >> values: many0!(enum_value) >> many0!(br) >> tag!("}") >> many0!(br)
            >> many0!(tag!(";")) >> (Enumeration {
            name: name,
            values: values,
        })
    )
);

named!(
    option_ignore<()>,
    do_parse!(tag!("option") >> many1!(br) >> take_until_and_consume!(";") >> ())
);

named!(
    service_ignore<()>,
    do_parse!(
        tag!("service") >> many1!(br) >> word >> many0!(br) >> tag!("{")
            >> take_until_and_consume!("}") >> ()
    )
);

enum Event {
    Syntax(Syntax),
    Import(String),
    Package(String),
    Message(Message),
    Enum(Enumeration),
    Extensions(Vec<Extension>),
    Ignore,
}

named!(
    event<Event>,
    alt!(syntax => { |s| Event::Syntax(s) } |
            import => { |i| Event::Import(i) } |
            package => { |p| Event::Package(p) } |
            message => { |m| Event::Message(m) } |
            enumerator => { |e| Event::Enum(e) } |
            extensions => { |e| Event::Extensions(e) } |
            option_ignore => { |_| Event::Ignore } |
            service_ignore => { |_| Event::Ignore } |
            br => { |_| Event::Ignore })
);

named!(pub file_descriptor<FileDescriptor>,
       map!(many0!(event), |events: Vec<Event>| {
           let mut desc = FileDescriptor::default();
           for event in events {
               match event {
                   Event::Syntax(s) => desc.syntax = s,
                   Event::Import(i) => desc.import_paths.push(i),
                   Event::Package(p) => desc.package = p,
                   Event::Message(m) => desc.messages.push(m),
                   Event::Enum(e) => desc.enums.push(e),
                   Event::Extensions(e) => desc.extensions.extend(e),
                   Event::Ignore => (),
               }
           }
           desc
       }));

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_message() {
        let msg = r#"message ReferenceData
    {
        repeated ScenarioInfo  scenarioSet = 1;
        repeated CalculatedObjectInfo calculatedObjectSet = 2;
        repeated RiskFactorList riskFactorListSet = 3;
        repeated RiskMaturityInfo riskMaturitySet = 4;
        repeated IndicatorInfo indicatorSet = 5;
        repeated RiskStrikeInfo riskStrikeSet = 6;
        repeated FreeProjectionList freeProjectionListSet = 7;
        repeated ValidationProperty ValidationSet = 8;
        repeated CalcProperties calcPropertiesSet = 9;
        repeated MaturityInfo maturitySet = 10;
    }"#;

        let mess = message(msg.as_bytes());
        if let ::nom::IResult::Done(_, mess) = mess {
            assert_eq!(10, mess.fields.len());
        }
    }

    #[test]
    fn test_enum() {
        let msg = r#"enum PairingStatus {
                DEALPAIRED        = 0;
                INVENTORYORPHAN   = 1;
                CALCULATEDORPHAN  = 2;
                CANCELED          = 3;
    }"#;

        let enumeration = enumerator(msg.as_bytes());
        if let ::nom::IResult::Done(_, mess) = enumeration {
            assert_eq!(4, mess.values.len());
        }
    }

    #[test]
    fn test_ignore() {
        let msg = r#"option optimize_for = SPEED;"#;

        match option_ignore(msg.as_bytes()) {
            ::nom::IResult::Done(_, _) => (),
            e => panic!("Expecting done {:?}", e),
        }
    }

    #[test]
    fn test_import() {
        let msg = r#"syntax = "proto3";

    import "test_import_nested_imported_pb.proto";

    message ContainsImportedNested {
        optional ContainerForNested.NestedMessage m = 1;
        optional ContainerForNested.NestedEnum e = 2;
    }
    "#;
        let desc = file_descriptor(msg.as_bytes()).to_full_result().unwrap();
        assert_eq!(
            vec!["test_import_nested_imported_pb.proto"],
            desc.import_paths
        );
    }

    #[test]
    fn test_package() {
        let msg = r#"
        package foo.bar;

    message ContainsImportedNested {
        optional ContainerForNested.NestedMessage m = 1;
        optional ContainerForNested.NestedEnum e = 2;
    }
    "#;
        let desc = file_descriptor(msg.as_bytes()).to_full_result().unwrap();
        assert_eq!("foo.bar".to_string(), desc.package);
    }

    #[test]
    fn test_nested_message() {
        let msg = r#"message A
    {
        message B {
            repeated int32 a = 1;
            optional string b = 2;
        }
        optional b = 1;
    }"#;

        let mess = message(msg.as_bytes());
        if let ::nom::IResult::Done(_, mess) = mess {
            assert!(mess.messages.len() == 1);
        }
    }

    #[test]
    fn test_map() {
        let msg = r#"message A
    {
        optional map<string, int32> b = 1;
    }"#;

        let mess = message(msg.as_bytes());
        if let ::nom::IResult::Done(_, mess) = mess {
            assert_eq!(1, mess.fields.len());
            match mess.fields[0].typ {
                FieldType::Map(ref f) => match &**f {
                    &(FieldType::String, FieldType::Int32) => (),
                    ref f => panic!("Expecting Map<String, Int32> found {:?}", f),
                },
                ref f => panic!("Expecting map, got {:?}", f),
            }
        } else {
            panic!("Could not parse map message");
        }
    }

    #[test]
    fn test_oneof() {
        let msg = r#"message A
    {
        optional int32 a1 = 1;
        oneof a_oneof {
            string a2 = 2;
            int32 a3 = 3;
            bytes a4 = 4;
        }
        repeated bool a5 = 5;
    }"#;

        let mess = message(msg.as_bytes());
        if let ::nom::IResult::Done(_, mess) = mess {
            assert_eq!(1, mess.oneofs.len());
            assert_eq!(3, mess.oneofs[0].fields.len());
        }
    }

    #[test]
    fn test_reserved() {
        let msg = r#"message Sample {
       reserved 4, 15, 17 to 20, 30;
       reserved "foo", "bar";
       uint64 age =1;
       bytes name =2;
    }"#;

        let mess = message(msg.as_bytes());
        if let ::nom::IResult::Done(_, mess) = mess {
            assert_eq!(vec![4..5, 15..16, 17..21, 30..31], mess.reserved_nums);
            assert_eq!(
                vec!["foo".to_string(), "bar".to_string()],
                mess.reserved_names
            );
            assert_eq!(2, mess.fields.len());
        } else {
            panic!("Could not parse reserved fields message");
        }
    }

    #[test]
    fn test_default_value_int() {
        let msg = r#"message Sample {
            optional int32 x = 1 [default = 17];
        }"#;

        let mess = message(msg.as_bytes()).unwrap().1;
        assert_eq!("17", mess.fields[0].default.as_ref().expect("default"));
    }

    #[test]
    fn test_default_value_string() {
        let msg = r#"message Sample {
            optional string x = 1 [default = "ab\nc d\"g\'h\0\"z"];
        }"#;

        let mess = message(msg.as_bytes()).unwrap().1;
        assert_eq!(r#""ab\nc d\"g\'h\0\"z""#, mess.fields[0].default.as_ref().expect("default"));
    }

    #[test]
    fn test_default_value_bytes() {
        let msg = r#"message Sample {
            optional bytes x = 1 [default = "ab\nc d\xfeE\"g\'h\0\"z"];
        }"#;

        let mess = message(msg.as_bytes()).unwrap().1;
        assert_eq!(r#""ab\nc d\xfeE\"g\'h\0\"z""#, mess.fields[0].default.as_ref().expect("default"));
    }

    #[test]
    fn test_group() {
        let msg = r#"message MessageWithGroup {
            optional string aaa = 1;

            repeated group Identifier = 18 {
                optional int32 iii = 19;
                optional string sss = 20;
            }

            required int bbb = 3;
        }"#;
        let mess = message(msg.as_bytes()).unwrap().1;

        assert_eq!("Identifier", mess.fields[1].name);
        if let FieldType::Group(ref group_fields) = mess.fields[1].typ {
            assert_eq!(2, group_fields.len());
        } else {
            panic!("expecting group");
        }

        assert_eq!("bbb", mess.fields[2].name);
    }

    #[test]
    fn test_incorrect_file_descriptor() {
        let msg = r#"
            message Foo {}

            dfgdg
        "#;

        assert!(FileDescriptor::parse(msg.as_bytes()).is_err());
    }

    #[test]
    fn test_extend() {
        let proto = r#"
            syntax = "proto2";

            extend google.protobuf.FileOptions {
                optional bool foo = 17001;
                optional string bar = 17002;
            }

            extend google.protobuf.MessageOptions {
                optional bool baz = 17003;
            }
        "#;

        let fd = FileDescriptor::parse(proto.as_bytes()).expect("fd");
        assert_eq!(3, fd.extensions.len());
        assert_eq!("google.protobuf.FileOptions", fd.extensions[0].extendee);
        assert_eq!("google.protobuf.FileOptions", fd.extensions[1].extendee);
        assert_eq!("google.protobuf.MessageOptions", fd.extensions[2].extendee);
        assert_eq!(17003, fd.extensions[2].field.number);
    }
}
