use core::fmt::Write;
use std::borrow::ToOwned;
use std::format;
use std::string::{String, ToString};
use std::vec::Vec;
use postcard_schema::Schema;
use postcard_schema::schema::{DataModelType, DataModelVariant, NamedType, NamedValue, NamedVariant};
use crate::command::Command;
use crate::DeviceInfo;
use crate::event::Event;

const TEMPLATE: &str = include_str!("wireshark/template.lua");
const MARKER: &str = "-- @@GENERATED@@\n";

pub fn lua() -> String {
    let mut generator = Generator::default();
    let mut schema = String::from("local SCHEMA = {\n");
    for root in [Command::SCHEMA, Event::SCHEMA, DeviceInfo::SCHEMA] {
        let node = generator.node(root, &snake_case(root.name), root.name, 1);
        writeln!(schema, "    {} = {node},", root.name).unwrap();
    }
    schema.push_str("}\n");

    let mut fields = String::from("local fields = {\n");
    for (abbrev, declaration) in &generator.fields {
        writeln!(fields, "    [\"{abbrev}\"] = {declaration},").unwrap();
    }
    fields.push_str("}\n");

    let (head, tail) = TEMPLATE
        .split_once(MARKER)
        .expect("template lacks the generated marker");
    format!("{head}{fields}\n{schema}{tail}")
}

#[derive(Default)]
struct Generator {
    fields: Vec<(String, String)>,
}

impl Generator {
    fn field(&mut self, path: &str, declaration: String) {
        match self.fields.iter().find(|(abbrev, _)| abbrev == path) {
            Some((_, existing)) => {
                assert_eq!(*existing, declaration, "conflicting fields for {path}")
            }
            None => self.fields.push((path.to_owned(), declaration))
        }
    }

    fn leaf(&mut self, kind: &str, protofield: &str, path: &str, name: &str) -> String {
        let declaration = format!("ProtoField.{protofield}(\"faultydeck.{path}\", \"{name}\")");
        self.field(path, declaration);
        format!("{{ k = \"{kind}\", field = \"{path}\" }}")
    }

    fn varint(&mut self, ty: &NamedType, protofield: &str, path: &str, name: &str) -> String {
        let zigzag = if protofield.starts_with("int") {
            ", zigzag = true"
        } else {
            ""
        };
        let declaration = format!("ProtoField.{protofield}(\"faultydeck.{path}\", \"{name}\")");
        self.field(path, declaration);
        format!(
            "{{ k = \"varint\", ty = \"{}\"{zigzag}, field = \"{path}\" }}",
            ty.name
        )
    }

    fn node(&mut self, ty: &NamedType, path: &str, name: &str, indent: usize) -> String {
        match ty.ty {
            DataModelType::Bool => self.leaf("bool", "bool", path, name),
            DataModelType::I8 => self.leaf("i8", "int8", path, name),
            DataModelType::U8 => self.leaf("u8", "uint8", path, name),
            DataModelType::I16 => self.varint(ty, "int16", path, name),
            DataModelType::I32 => self.varint(ty, "int32", path, name),
            DataModelType::I64 => self.varint(ty, "int64", path, name),
            DataModelType::U16 => self.varint(ty, "uint16", path, name),
            DataModelType::U32 => self.varint(ty, "uint32", path, name),
            DataModelType::U64 => self.varint(ty, "uint64", path, name),
            DataModelType::String => self.leaf("string", "string", path, name),
            DataModelType::ByteArray => self.byte_seq(path, name),
            DataModelType::Unit | DataModelType::UnitStruct => "{ k = \"unit\" }".to_string(),
            DataModelType::Option(inner) => {
                let inner = self.node(inner, path, name, indent + 1);
                block(indent, &[("k", "\"option\"".into()), ("of", inner)])
            }
            DataModelType::NewtypeStruct(inner) => {
                let inner = self.node(inner, path, name, indent + 1);
                block(indent, &[("k", "\"newtype\"".into()), ("of", inner)])
            }
            DataModelType::Seq(inner) if *inner.ty == DataModelType::U8 => {
                self.byte_seq(path, name)
            }
            DataModelType::Seq(inner) => {
                let inner = self.node(inner, path, name, indent + 1);
                block(indent, &[("k", "\"seq\"".into()), ("of", inner)])
            }
            DataModelType::Tuple(items) | DataModelType::TupleStruct(items) => {
                self.tuple(items, path, name, indent)
            }
            DataModelType::Struct(values) => self.structure(values, path, indent),
            DataModelType::Enum(variants) => self.enumeration(variants, path, name, indent),
            other => panic!("{} ({other:?}) has no Lua decoder", ty.name),
        }
    }

    fn byte_seq(&mut self, path: &str, name: &str) -> String {
        let declaration = format!("ProtoField.bytes(\"faultydeck.{path}\", \"{name}\")");
        self.field(path, declaration);
        format!("{{ k = \"seq\", of = {{ k = \"u8\" }}, field = \"{path}\" }}")
    }

    fn tuple(&mut self, items: &[&NamedType], path: &str, name: &str, indent: usize) -> String{
        if !items.is_empty() && items.iter().all(|item| *item.ty == DataModelType::U8) {
            self.field(path, format!("ProtoField.bytes(\"faultydeck.{path}\", \"{name}\")"));
            return format!(
                "{{ k = \"bytes\", n = {}, field = \"{path}\" }}",
                items.len()
            );
        }
        let homogeneous = items.iter().all(|item| item == &items[0]);
        let rendered: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let item_path = if homogeneous {
                    path.to_string()
                } else {
                    format!("{path}.{i}")
                };
                self.node(item, &item_path, name, indent + 2)
            })
            .collect();

        block(
            indent,
            &[
                ("k", "\"tuple\"".into()),
                ("items", list(indent + 1, &rendered)),
            ],
        )
    }

    fn structure(&mut self, values: &[&NamedValue], path: &str, indent: usize) -> String {
        let rendered: Vec<String> = values
            .iter()
            .map(|value| {
                let node = self.node(
                    value.ty,
                    &format!("{path}.{}", value.name),
                    value.name,
                    indent + 2
                );
                format!("{{ name = \"{}\", node = {node} }}", value.name)
            })
            .collect();
        block(
            indent,
            &[
                ("k", "\"struct\"".into()),
                ("fields", list(indent + 1, &rendered)),
            ]
        )
    }

    fn enumeration(&mut self, variants: &[&NamedVariant], path: &str, name: &str, indent: usize) -> String {
        let names: Vec<String> = variants
            .iter()
            .enumerate()
            .map(|(tag, v)| format!("[{tag}] = \"{}\"", v.name))
            .collect();
        let declaration = format!(
            "ProtoField.uint32(\"faultydeck.{path}\", \"{name}\", base.DEC, {{ {} }})",
            names.join(", ")
        );
        self.field(path, declaration);
        let mut rendered = Vec::new();
        for (tag, variant) in variants.iter().enumerate() {
            let variant_path = format!("{path}.{}", snake_case(variant.name));
            let payload = match variant.ty {
                DataModelVariant::UnitVariant => None,
                DataModelVariant::NewtypeVariant(inner) => {
                    Some(self.node(inner, &variant_path, variant.name, indent + 3))
                }
                DataModelVariant::TupleVariant(items) => {
                    Some(self.tuple(items, &variant_path, variant.name, indent + 3))
                }
                DataModelVariant::StructVariant(values) => {
                    Some(self.structure(values, &variant_path, indent + 3))
                }
            };
            let name = format!("\"{}\"", variant.name);
            rendered.push(match payload {
                Some(node) => format!(
                    "[{tag}] = {}",
                    block(indent + 2, &[("name", name), ("node", node)])
                ),
                None => format!("[{tag}] = {{ name = {name} }}")
            });
        }
        block(
            indent,
            &[
                ("k", "\"enum\"".into()),
                ("field", format!("\"{path}\"")),
                ("variants", list(indent + 1, &rendered)),
            ]
        )
    }
}

fn block(indent: usize, entries: &[(&str, String)]) -> String {
    let pad = "    ".repeat(indent + 1);
    let mut out = String::from("{\n");
    for (key, value) in entries {
        writeln!(out, "{pad}{key} = {value},").unwrap();
    }
    out + &"    ".repeat(indent) + "}"
}

fn list(indent: usize, items: &[String]) -> String {
    let pad = "    ".repeat(indent + 1);
    let mut out = String::from("{\n");
    for item in items {
        writeln!(out, "{pad}{item},").unwrap();
    }
    out + &"    ".repeat(indent) + "}"
}

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && i != 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn checked_in_dissector_is_up_to_date() {
        let checked_in = include_str!("../../../tools/wireshark/faulty-deck.lua");
        assert!(
            checked_in == super::lua(),
            "tools/wireshark/faulty-deck.lua is stale; regenerate it with `just host::wireshark`"
        );
    }
}