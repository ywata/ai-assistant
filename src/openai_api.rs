use crate::Prompt;
use async_openai::config::Config;
use async_openai::{
    config::{AzureConfig, OpenAIConfig},
    error::OpenAIError,
    types::{
        AssistantObject, CreateAssistantRequestArgs, CreateMessageRequestArgs,
        CreateRunRequestArgs, CreateThreadRequestArgs, MessageContent, RunStatus, ThreadObject,
    },
    Client,
};
use std::collections::HashMap;
use std::fmt::Debug;

use std::sync::Arc;

use crate::OpenAIApiError::OpenAIAccessError;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone)]
pub enum OpenAi {
    OpenAiToken {
        token: String,
        model: String,
    },
    AzureAiToken {
        key: String,
        endpoint: String,
        deployment_id: String,
        api_version: String,
    },
}

impl Debug for OpenAi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAi::OpenAiToken { model, .. } => {
                write!(f, "OpenAiToken {{ token: **** model: {} }}", model)
            }
            OpenAi::AzureAiToken {
                endpoint,
                deployment_id,
                api_version,
                ..
            } => write!(
                f,
                "AzureAiToken {{ key: ****, endpoint: {}, deployment_id: {}, api_version: {} }}",
                endpoint, deployment_id, api_version
            ),
        }
    }
}

impl OpenAi {
    fn get_model(&self) -> String {
        match self {
            OpenAi::OpenAiToken { model, .. } => model.clone(),
            OpenAi::AzureAiToken { .. } => "".to_string(),
        }
    }
}

impl Default for OpenAi {
    fn default() -> Self {
        OpenAi::OpenAiToken {
            token: "".to_string(),
            model: "".to_string(),
        }
    }
}

pub type AssistantName = String;
#[derive(Clone, Debug)]
pub struct Assistant {
    thread: ThreadObject,
    assistant: AssistantObject,
}

#[derive(Clone, Debug)]
pub struct Context {
    #[cfg(not(feature = "azure_ai"))]
    client: Client<OpenAIConfig>,
    #[cfg(feature = "azure_ai")]
    client: Client<AzureConfig>,
    assistants: HashMap<AssistantName, Assistant>,
}

#[cfg(not(feature = "azure_ai"))]
pub type CClient = Client<OpenAIConfig>;
#[cfg(feature = "azure_ai")]
pub type CClient = Client<AzureConfig>;

impl Context {
    pub fn new(client: CClient) -> Context {
        Context {
            client,
            assistants: HashMap::new(),
        }
    }
    pub fn add_assistant(&mut self, name: &String, assistant: Assistant) {
        self.assistants.insert(name.clone(), assistant);
    }
}

pub async fn connect(
    config: OpenAi,
    client: CClient,
    names: Vec<String>,
    prompts: HashMap<String, Box<Prompt>>,
) -> Result<Context, OpenAIApiError> {
    let mut context: Context = Context::new(client);
    let mut connection_setupped = false;
    for key in names {
        if let Some(prompt) = prompts.get(&key) {
            info!("Setting up assistant for {}", &key);
            let (thread, assistant) =
                setup_assistant(&config, &context.client, &key, &prompt.instruction).await?;
            context.add_assistant(&key, Assistant { thread, assistant });
            connection_setupped = true;
        }
    }
    if connection_setupped {
        Ok(context)
    } else {
        Err(OpenAIAccessError)
    }
}

async fn setup_assistant(
    config: &OpenAi,
    client: &CClient,
    name: &str,
    prompt: &str,
) -> Result<(ThreadObject, AssistantObject), OpenAIApiError> {
    //create a thread for the conversation
    let thread_request = CreateThreadRequestArgs::default().build()?;
    let thread = client.threads().create(thread_request.clone()).await?;

    let assistant_name = name;
    let instructions = prompt;
    let model = config.get_model();

    //create the assistant
    let assistant_request = CreateAssistantRequestArgs::default()
        .name(assistant_name)
        .instructions(instructions)
        .model(&model)
        .build()?;
    let assistant = client.assistants().create(assistant_request).await?;
    //get the id of the assistant

    Ok((thread, assistant))
}

pub async fn ask(
    context: Arc<Mutex<Context>>,
    name: String,
    tag: String,
    input: String,
) -> Result<(String, String, String), (String, OpenAIApiError)> {
    let query = [("limit", "1")]; //limit the list responses to 1 message

    // TODO: handle locked state
    let ctx = context.lock().await;
    let client = ctx.client.clone();

    if let Some(interaction) = ctx.assistants.get(&name) {
        let assistant_id = interaction.assistant.id.clone();
        let thread_id = interaction.thread.id.clone();

        //create a message for the thread
        let message = CreateMessageRequestArgs::default()
            .role("user")
            .content(input.clone())
            .build()
            .map_err(|e| (name.clone(), e.into()))?;
        debug!("Create message request args: {:#?}", message);
        //attach message to the thread
        let _message_obj = client
            .threads()
            .messages(&thread_id)
            .create(message)
            .await
            .map_err(|_| (name.clone(), OpenAIApiError::OpenAIAccessError))?;
        debug!("messagne created");
        //create a run for the thread
        let run_request = CreateRunRequestArgs::default()
            .assistant_id(assistant_id)
            .build()
            .map_err(|e| (name.clone(), e.into()))?;
        let run = client
            .threads()
            .runs(&thread_id)
            .create(run_request)
            .await
            .map_err(|e| (name.clone(), e.into()))?;
        debug!("Start waiting for response");
        //wait for the run to complete
        let mut awaiting_response = true;
        while awaiting_response {
            //retrieve the run
            let run = client
                .threads()
                .runs(&thread_id)
                .retrieve(&run.id)
                .await
                .map_err(|e| (name.clone(), e.into()))?;
            //check the status of the run
            match run.status {
                RunStatus::Completed => {
                    awaiting_response = false;
                    // once the run is completed we
                    // get the response from the run
                    // which will be the first message
                    // in the thread

                    //retrieve the response from the run
                    let response = client
                        .threads()
                        .messages(&thread_id)
                        .list(&query)
                        .await
                        .map_err(|e| (name.clone(), e.into()))?;
                    //get the message id from the response
                    let message_id = response.data.first().unwrap().id.clone();
                    //get the message from the response
                    let message = client
                        .threads()
                        .messages(&thread_id)
                        .retrieve(&message_id)
                        .await
                        .map_err(|e| (name.clone(), e.into()))?;
                    //get the content from the message
                    let content = message.content.first().unwrap();

                    //get the text from the content
                    let text = match content {
                        MessageContent::Text(text) => text.text.value.clone(),
                        MessageContent::ImageFile(_) => {
                            panic!("imaged are not supported in the terminal")
                        }
                    };
                    //print the text
                    info!("--- Response: {}", &text);
                    return Ok((name, tag, text.clone()));
                }

                RunStatus::Failed => {
                    awaiting_response = false;
                    error!("--- Run Failed: {:#?}", run);
                }

                otherwise => report_status(otherwise),
            }
        }
    } else {
        panic!("No interaction found");
    }

    Ok((name, tag, String::from("???")))
}

pub trait AiService<C: Config> {
    fn create_client(&self) -> Option<Client<C>>;
}

impl AiService<OpenAIConfig> for OpenAi {
    fn create_client(&self) -> Option<Client<OpenAIConfig>> {
        info!("Creating openai client");
        match self {
            OpenAi::OpenAiToken { token, .. } => {
                let token = token.as_str();
                let oai_config: OpenAIConfig = OpenAIConfig::default().with_api_key(token);

                //create a client

                Some(Client::with_config(oai_config))
            }
            _ => None,
        }
    }
}

impl AiService<AzureConfig> for OpenAi {
    fn create_client(&self) -> Option<Client<AzureConfig>> {
        info!("Creating azure client");
        match self {
            OpenAi::AzureAiToken {
                key,
                endpoint,
                deployment_id,
                api_version,
            } => {
                let azure_config: AzureConfig = AzureConfig::default()
                    .with_api_key(key)
                    //with_endpoint(endpoint)
                    .with_api_base(endpoint)
                    .with_deployment_id(deployment_id)
                    .with_api_version(api_version);

                //create a client
                Some(Client::with_config(azure_config))
            }
            _ => None,
        }
    }
}

pub fn report_status(status: RunStatus) {
    match status {
        RunStatus::Queued => {
            info!("--- Run Queued");
        }
        RunStatus::Cancelling => {
            info!("--- Run Cancelling");
        }
        RunStatus::Cancelled => {
            info!("--- Run Cancelled");
        }
        RunStatus::Expired => {
            info!("--- Run Expired");
        }
        RunStatus::RequiresAction => {
            info!("--- Run Requires Action");
        }
        RunStatus::InProgress => {
            info!("--- Waiting for response...");
        }
        _ => panic!("should not reach here"),
    }
}

#[derive(Debug, Clone)]
pub enum OpenAIApiError {
    OpenAIAccessError,
}

impl From<OpenAIError> for OpenAIApiError {
    fn from(error: OpenAIError) -> OpenAIApiError {
        dbg!(error);
        OpenAIAccessError
    }
}

impl From<std::io::Error> for OpenAIApiError {
    fn from(error: std::io::Error) -> OpenAIApiError {
        dbg!(error);
        OpenAIApiError::OpenAIAccessError
    }
}
