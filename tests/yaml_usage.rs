use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct OpenAIConfig {
    token: String,
}

#[test]
fn test_serde_yaml_from_str() {
    let yaml_string = r#"
    openai:
      token: "The token"
    "#;
    let map: Result<Value, serde_yaml::Error> = serde_yaml::from_str(yaml_string);
    assert!(map.is_ok());
    let val = map.unwrap();
    assert_eq!(val["openai"]["token"], "The token");
}

#[test]
fn test_serde_yaml_from_file() {
    let yaml_str = r#"
    foo:
      bar:1
    "#;
    let _value = serde_yaml::from_str(yaml_str).map(|_v: Value| ());
}

#[test]
fn test_serde_serialize() {
    let pair = (1, "hello", "world");
    let serialized = serde_yaml::to_string(&pair).unwrap();
    assert_eq!(serialized, "- 1\n- hello\n- world\n");
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Inner {
    j: i32,
}
#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Outer {
    i: i32,
    inner: Inner,
}

#[test]
fn test_serde_serialize_struct() {
    let outer = Outer {
        i: 10,
        inner: Inner { j: 20 },
    };
    let serialized = serde_yaml::to_string(&outer).unwrap();
    assert_eq!(serialized, "i: 10\ninner:\n  j: 20\n");
    let deserialized = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(outer, deserialized);
}
