use std::env;
use strum_macros::{Display, EnumIter, EnumString};
use thiserror::Error;
use crate::config::ConfigError::{ConversionFailed, EnvNotFound, UnexpectedKey};
use serde::{Deserialize};
use std::collections::BTreeMap;
use std::fmt::Debug;

#[derive(Error, Deserialize, Debug, PartialEq)]
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
    fn read_from_env(key:&str) -> Result<T, ConfigError>;

}

pub fn convert<T:for<'a>Deserialize<'a>+Clone + std::fmt::Debug>(key:Option<&String>, yaml_string: &str)
    -> Result<T, ConfigError> {
    match key {
        None => {
            serde_yaml::from_str(yaml_string).map_err(|_| ConversionFailed)
        },
        Some(key) => {
            let config: Result<BTreeMap<String, T>, _>
                = serde_yaml::from_str(yaml_string);
            println!("{:?}", &config);
            let map = config.map_err(|_| UnexpectedKey)?;
            map.get(key).cloned().ok_or(ConversionFailed)
        },
    }
}


pub fn read_config<T:for<'a>Deserialize<'a>+Clone+ std::fmt::Debug>(key:Option<&String>, contents: &str) -> Result<T, ConfigError>{
    convert::<T>(key, contents)
}

pub fn get_env<T:for<'a>Deserialize<'a> + Clone>(key:&str, name: &str) -> Result<T, ConfigError> {
    let mut env_name = String::from(key);
    env_name.push('_');
    env_name.push_str(name);
    let env_var = env::var(env_name).map_err(|_| EnvNotFound);
    let result: Result<T, ConfigError>
        = env_var.and_then(|str| serde_yaml::from_str(&str).map_err(|_| UnexpectedKey));
    result
}



#[cfg(test)]
mod test{
    use super::*;
    use serde::{Serialize, Deserialize};
    #[test]
    fn test_convert() {
        #[derive(Deserialize, Debug, Clone, PartialEq)]
        enum TestConfig {
            Tag1{user:String},
            Tag2{email:String},
        }
        let input = r#"
        key:
           !Tag1
           user: someone
        "#.to_string();
        let res = convert::<TestConfig>(Some(&"key".to_string()), &input);
        assert_eq!(res, Ok(TestConfig::Tag1{user: "someone".to_string()}));
    }
    #[test]
    fn test_convert_no_key() {
        #[derive(Deserialize, Debug, Clone, PartialEq)]
        enum TestConfig {
            Tag1{user:String},
            Tag2{email:String},
        }
        let input = r#"
        !Tag1
        user: someone
        "#.to_string();
        let res = convert::<TestConfig>(None, &input);
        assert_eq!(res, Ok(TestConfig::Tag1{user: "someone".to_string()}));
    }
    #[test]
    fn test_convert_fail() {
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
        let res = convert::<TestConfig>(Some(&"key".to_string()), &input);
        assert_eq!(res, Err(ConfigError::ConversionFailed));
    }
    #[test]
    fn test_convert_no_key_fail() {
        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
        enum TestConfig {
            Tag1{user:String},
            Tag2{email:String}
        }
        let input = r#"
        !Tag1
        password: asdfasdf
        "#.to_string();
        let res = convert::<TestConfig>(None, &input);
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
        let res = convert::<TestConfig>(Some(&"key".to_string()), &input);
        assert_eq!(res, Err(ConfigError::UnexpectedKey));
    }

}
