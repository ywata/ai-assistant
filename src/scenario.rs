use std::collections::HashMap;
use serde::{Deserialize};


#[derive(Clone, Debug, Default, Deserialize)]
pub struct Input{
    pub tag: String,
    pub prefix: Option<String>,
    pub text: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Prompt {
    pub instruction: String,
    pub inputs: Vec<Input>,
}


impl Prompt {
    pub fn new(instruction: String, inputs: Vec<Input>) -> Self {
        Prompt { instruction, inputs }
    }
    pub fn get_instruction(&self) -> String {
        self.instruction.clone()
    }
    pub fn get_inputs(&self) -> Vec<Input> {
        self.inputs.clone()
    }
}


#[derive(Clone, Debug, Deserialize, PartialEq)]
//#[serde(tag = "type")]
//#[serde(tag = "type", content = "details")]
//#[serde(untagged)]
pub enum Directive {
    KeepAsIs,
    PassResultTo { name: String, tag: String },
    JumpTo { name: String, tag: String },
}
#[derive(Clone, Debug, Deserialize)]
pub struct Workflow{workflow:HashMap<String, HashMap<String, Directive>>}


impl Workflow {
    pub fn new(workflow:HashMap<String, HashMap<String, Directive>>) -> Self {
        Workflow { workflow }
    }
    pub fn get_directive(&self, name: &str, tag: &str) -> Directive {
        self.workflow.get(name)
            .and_then(|x| x.get(tag))
            .unwrap_or(&Directive::KeepAsIs)
            .clone()
    }
}

impl Default for Workflow {
    fn default() -> Self {
        Workflow { workflow: HashMap::new() }
    }
}
