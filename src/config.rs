use std::env;
use strum_macros::{Display, EnumIter, EnumString};
use thiserror::Error;
use crate::config::ConfigError::{ConversionFailed, EnvNotFound, UnexpectedKey};
use serde::{Deserialize};
use std::collections::BTreeMap;
use std::fmt::Debug;

#[derive(Error, Debug, PartialEq)]
pub enum ConfigError {
    #[error("unexpected entry in template")]
    UnexpectedKey,
    #[error("mandatory item missing")]
    MissingMandatoryKey,
    #[error("no enviroment variable found")]
    EnvNotFound,
    #[error("conversion failed")]
    ConversionFailed,
}


#[derive( Display, EnumIter, EnumString, PartialEq, Debug)]
pub enum Enforce {
    Mandatory,
    Optional
}

pub trait ReadFromEnv<T:for<'a>Deserialize<'a>+Clone>{
    fn read_from_env(key:&String) -> Result<T, ConfigError>;

}

pub fn convert<T:for<'a>Deserialize<'a>+Clone>(key:&String, yaml_string: &String) -> Result<T, ConfigError> {
    let config: Result<BTreeMap<String, T>, ConfigError>
        = serde_yaml::from_str(yaml_string).or_else(|_|Err(UnexpectedKey));
    let map = config?;

    map.get(key).cloned().ok_or(ConversionFailed)
}


pub fn read_config<T:for<'a>Deserialize<'a>+Clone>(key:&String, contents: &String) -> Result<T, ConfigError>{
    convert::<T>(key, contents)
}

pub fn get_env<T:for<'a>Deserialize<'a> + Clone>(key:&String, name: &String) -> Result<T, ConfigError> {
    let mut env_name = String::from(key);
    env_name.push('_');
    env_name.push_str(name);
    let env_var = env::var(env_name)
        .or_else(|_|Err(EnvNotFound));
    let result: Result<T, ConfigError>
        = env_var.and_then(|str| serde_yaml::from_str(&str).or_else(|_|Err(UnexpectedKey)));
    result
}



#[cfg(test)]
mod test{
    use super::*;
    use serde::{Serialize, Deserialize};
    #[test]
    fn test_convert(){
        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
        enum TestConfig {
            Tag1{user:String},
            Tag2{email:String}
        }
        let input = r#"
        key:
           !Tag1
           user: someone
        "#.to_string();
        let res = convert::<TestConfig>(&input, &"key".to_string());
        assert_eq!(res, Ok(TestConfig::Tag1{user: "someone".to_string()}));
    }
    #[test]
    fn test_convert_fail(){
        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
        enum TestConfig {
            Tag1{user:String},
            Tag2{email:String}
        }
        let input = r#"
        keyword:
           !Tag1
           user: someone
           password: asdfasdf
        "#.to_string();
        let res = convert::<TestConfig>(&input, &"key".to_string());
        assert_eq!(res, Err(ConfigError::ConversionFailed));
    }
    #[test]
    fn test_convert_yaml_fail(){
        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
        enum TestConfig {
            Tag1{user:String},
            Tag2{email:String}
        }
        let input = r#"
        key:
           !Tag1
           - user: someone
        "#.to_string();
        let res = convert::<TestConfig>(&input, &"key".to_string());
        assert_eq!(res, Err(ConfigError::UnexpectedKey));
    }

}
