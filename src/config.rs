use std::str::FromStr;
use std::error::Error;
use std::env;
use strum_macros::{Display, EnumIter, EnumString};
use serde_yaml::{Value, Mapping};
use thiserror::Error;
use crate::config::ConfigError::{EnvNotFound, UnexpectedKey};

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("unexpected entry in template")]
    UnexpectedKey,
    #[error("mandatory item missing")]
    MissingMandatoryKey,
    #[error("no enviroment variable found")]
    EnvNotFound,
}


#[derive( Display, EnumIter, EnumString, PartialEq, Debug)]
pub enum Enforce {
    Mandatory,
    Optional
}


pub fn read_config(key:&String, template:&String, contents: &String) -> Result<Mapping, Box<dyn Error>>{
    let config_value: Value = serde_yaml::from_str(&contents)?;
    let templ_value : Value = serde_yaml::from_str(&template)?;

    if let (Some(Some(vmap)), Some(Some(tmap)))
        = (config_value.get(key).map(|v|v.as_mapping()),templ_value.get(key).map(|v|v.as_mapping())) {
        for (k, enforced) in  tmap {
            let enforce_level = enforced.as_str().map(|v| Enforce::from_str(v));
            let enforce_level = match enforce_level {
                Some(Ok(Enforce::Mandatory)) => {
                    if vmap.get(k).is_none() {
                        return Err(Box::new(UnexpectedKey));
                    }
                },
                Some(Ok(Enforce::Optional)) => {
                    // Nothing need to be done here
                },
                None | Some(Err(_)) => {
                    return Err(Box::new(UnexpectedKey));
                },
            };
        }
        return Ok(vmap.clone());
    }
    Err(Box::new(UnexpectedKey))
}

pub fn read_env_config(key:&String, template:&String) -> Result<Mapping, Box<dyn Error>> {
    let templ_value : Value = serde_yaml::from_str(&template)?;
    let mut key_ = key.clone();
    key_.to_ascii_uppercase();
    let mut mapping = Mapping::new();
    if let Some(Some(tmap)) = templ_value.get(key).map(|v|v.as_mapping()) {
        for (k, enforced) in  tmap {
            let enforce_level = enforced.as_str().map(|v| Enforce::from_str(v));
            match enforce_level {
                Some(Ok(Enforce::Mandatory)) => {
                    let mut env_var = key_.clone();
                    let k_str = k.as_str()
                        .map(|s|{env_var.push_str(&s.to_string().to_ascii_uppercase()); env_var})
                        .ok_or(Box::new(UnexpectedKey))
                        .and_then(|name|env::var(name).map_err(|_| Box::new(EnvNotFound)))?;
                    mapping.insert(k.clone(), Value::String(k_str));
                },
                Some(Ok(Enforce::Optional)) => {
                    let mut env_var = key_.clone();
                    let k_str = k.as_str()
                        .map(|s|{env_var.push_str(&s.to_string().to_ascii_uppercase()); env_var})
                        .ok_or(Box::new(UnexpectedKey))
                        .and_then(|name|env::var(name).map_err(|_| Box::new(EnvNotFound)));
                    if k_str.is_ok() {
                        mapping.insert(k.clone(), Value::String(k_str.unwrap()));
                    }
                },
                None | Some(Err(_)) => {
                    return Err(Box::new(UnexpectedKey));
                },
            }
        }
    }
    Ok(mapping)
}

#[cfg(test)]
mod test{
    #[test]
    fn test_read_config(){

    }
}
