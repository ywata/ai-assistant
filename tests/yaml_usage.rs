use std::fs;

use serde::{Serialize, Deserialize};
use serde_yaml::{Value, Error};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct OpenAIConfig{
    token: String
}

#[test]
fn test_serde_yaml_from_str(){
    let yaml_string = r#"
    openai:
      token: "The token"
    "#;
    let map : Result<Value, serde_yaml::Error> = serde_yaml::from_str(yaml_string);
    assert!(map.is_ok());
    let val = map.unwrap();
    assert_eq!(val["openai"]["token"], "The token");
}

#[test]
fn test_serde_yaml_from_file(){
    let yaml_str = r#"
    foo:
      bar:1
    "#;
    let value = serde_yaml::from_str(yaml_str)
        .and_then(|v:Value|Ok(()));
}