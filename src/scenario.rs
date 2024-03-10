use log::error;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Input {
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
        Prompt {
            instruction,
            inputs,
        }
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
    Stop,
}
#[derive(Clone, Debug, Deserialize, Default)]
pub struct Workflow {
    workflow: HashMap<String, HashMap<String, Directive>>,
}

impl Workflow {
    pub fn new(workflow: HashMap<String, HashMap<String, Directive>>) -> Self {
        Workflow { workflow }
    }
    pub fn get_directive(&self, name: &str, tag: &str) -> Directive {
        self.workflow
            .get(name)
            .and_then(|x| x.get(tag))
            .unwrap_or(&Directive::KeepAsIs)
            .clone()
    }
}

fn list_workflow_inputs(workflow: &Workflow) -> Vec<(String, String)> {
    let mut vec = Vec::new();
    for (_name, directives) in workflow.workflow.iter() {
        for (_person, directive) in directives.iter() {
            match directive {
                Directive::PassResultTo { name, tag, .. } => {
                    vec.push((name.clone(), tag.clone()));
                }
                Directive::JumpTo { name, tag } => {
                    vec.push((name.clone(), tag.clone()));
                }
                _ => {}
            }
        }
    }
    vec
}
fn list_input_identifier(prompts: &HashMap<String, Prompt>) -> Vec<(String, String)> {
    let mut vec = Vec::new();
    for (name, prompt) in prompts.iter() {
        for input in prompt.inputs.iter() {
            vec.push((name.clone(), input.tag.clone()));
        }
    }
    vec
}

// TODO: Implement the parse_scenario function. It does nothing at the moment.
pub fn parse_scenario(
    prompts: HashMap<String, Prompt>,
    workflow: Workflow,
) -> Option<(HashMap<String, Prompt>, Workflow)> {
    // As both prompts and workflow type checked successfully, what should do
    // here is to check
    // 1. if the workflow has a directive that points to a prompt in promts.

    let workflow_inputs = list_workflow_inputs(&workflow);
    let input_specifiers = list_input_identifier(&prompts);
    for (name, tag) in workflow_inputs.iter() {
        if !input_specifiers.contains(&(name.clone(), tag.clone())) {
            error!("({},{}) is not in {:?}", name, tag, input_specifiers);
            return None;
        }
    }
    Some((prompts, workflow))
}

// Check to see if key is in the list of input identifiers
pub fn parse_defined_key<'a>(
    prompts: &'a HashMap<String, Prompt>,
    key: &'a String,
) -> Option<String> {
    let filtered_keys: Vec<String> = list_input_identifier(prompts)
        .iter()
        .filter(|(name, _tag)| name == key)
        .map(|(_name, tag)| tag.clone())
        .collect();

    if filtered_keys.is_empty() {
        return None;
    }
    Some(key.clone())
}
