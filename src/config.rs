use std::str::FromStr;
use serde_yaml::{Value, Mapping};
use std::error::Error;
use std::io;
use std::io::ErrorKind;
use strum_macros::{Display, EnumIter, EnumString};
use std::collections::HashMap;

#[derive( Display, EnumIter, EnumString, PartialEq, Debug)]
pub enum Enforce {
    Mandatory,
    Optional
}


fn config_template(content: &String) -> Result<HashMap<String, Enforce>, Box<dyn Error>> {
    let mut template = HashMap::new();
    let yaml_data: HashMap<String, String> = serde_yaml::from_str(content)?;
    for (key, value) in yaml_data.iter() {
        let val = Enforce::from_str(value)?;
        template.insert(key.clone(), val);
    }

    Ok(template)
}

fn parse_template<'a>(content:&'a Mapping, template:&HashMap<String, Enforce>) -> Result<&'a Mapping, Box<dyn Error>> {
    for (key, value) in template.iter() {
        match value {
            Enforce::Mandatory => {
                if ! content.contains_key(key) {
                    let err = io::Error::new(ErrorKind::Other, "missing mandatory item");
                    let err_box = Box::new(err);
                    return Err(err_box);
                }
            },
            Enforce::Optional => (),
        }
    }
    Ok(content)
}

pub fn read_config(key:&String, template:&String, contents:&String) -> Result<Mapping, Box<dyn Error>>{
    let value: Value = serde_yaml::from_str(&contents)?;
    let templ = config_template(template)?;


    let key_ = key.clone();
    if value.is_mapping()
        & value.as_mapping().unwrap().contains_key(&key_)
        & value[&key_].is_mapping() {
        let result = parse_template(value[&key_].as_mapping().unwrap(), &templ)?;
        Ok(result.clone())
    }else{
        fn return_error() -> Result<Mapping, Box<dyn Error>> {
            let err = io::Error::new(io::ErrorKind::Other, "This is an error message");
            Err(Box::new(err))
        }
        return_error()
    }
}


#[cfg(test)]
mod test{
    #[test]
    fn test_read_config(){
        let res = crate::config::read_config(&"tests/test.yaml".to_string());


        assert!(res.is_ok());
        assert_eq!(res.unwrap()["openai"]["token"], "test-token");
    }
}
