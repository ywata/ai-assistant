use std::fs;
use serde_yaml::{from_str, Value};
use std::error::Error;

pub fn read_config(file_name:&String) -> Result<Value, Box<dyn Error>>{
    let contents = fs::read_to_string(file_name)?;
    let value: Value = serde_yaml::from_str(&contents)?;

    if value.is_mapping()
        & value.as_mapping().unwrap().contains_key("openai")
        & value["openai"].is_mapping()
        & value["openai"].as_mapping().unwrap().contains_key("token")
        & value["openai"]["token"].is_string() {
        Ok(value)
    }else{
        use std::error::Error;
        use std::io;

        fn return_error() -> Result<Value, Box<dyn Error>> {
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
        let res = crate::yaml_config::read_config(&"tests/test.yaml".to_string());


        assert!(res.is_ok());
        assert_eq!(res.unwrap()["openai"]["token"], "test-token");
    }
}
