use serde::{Deserialize};


#[derive(Clone, Debug, Default, Deserialize)]
pub struct Input{
    pub tag: String,
    pub text: String,
    pub include: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Prompt {
    pub instruction: String,
    pub inputs: Vec<Input>,
}

