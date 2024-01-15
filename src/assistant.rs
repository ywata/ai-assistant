use iced::futures;
use iced::widget::{self, column, container, image, row, text};
use iced::{
    Alignment, Application, Color, Command, Element, Length, Settings, Theme,
};

use clap::{Parser, Subcommand};


#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// yaml file to store credentials
    #[arg(long)]
    yaml: String,
    #[arg(long)]
    key: String,
    #[arg(long)]
    name: String,

    #[clap(subcommand)]
    command: Commands,
}


#[derive(Debug, Subcommand)]
enum Commands {
    AskAi {
        #[clap(
        required = true,
        //arg_enum,
        )]
        input: String,
        #[clap(
        required = true,
        //arg_enum,
        )]
        prompt: String,
        #[clap(
        required = true,
        //arg_enum,
        )]
        output_dir: String,
        #[arg(long)]
        markers: Option<Vec<String>>
    },
    RunFs {
        #[clap(
        required = true,
        //arg_enum,
        )]
        input: String,
        #[clap(
        required = true,
        //arg_enum,
        )]
        prompt: String,
        #[clap(
        required = true,
        //arg_enum,
        )]
        output_dir: String,
        #[arg(long)]
        markers: Option<Vec<String>>
    },
}

impl Default for Cli {
    fn default() -> Cli {
        <Cli as Default>::AskAi {
            input: "input/input.txt".to_string(),
            prompt:"input/prompt.txt".to_string(),
            output_dir:"output".to_string(),
            markers:None}

    }
}

#[derive(Debug, Clone, Default)]
enum LangFlag {
    English,
    Spanish,
    #[default]
    Franch,
}


impl Into<String> for LangFlag {
    fn into(self) -> String {
        match self {
            LangFlag::English => String::from("en"),
            LangFlag::Spanish => String::from("es"),
            LangFlag::Franch => String::from("fr"),
        }
    }
}

pub fn main() -> iced::Result {
    AiAssistant::run(Settings::default())
}


#[derive(Debug, Clone)]
enum PokedexState {
    Loading,
    Loaded { pokemon: Pokemon },
    Errored,
}


#[derive(Debug, Clone)]
struct AiAssistant {
    lang: LangFlag,
    state : PokedexState,
    env: Cli,
}


#[derive(Debug, Clone)]
enum Message {
    PokemonFound(Result<Pokemon, Error>),
    Search,
    Test,
}



impl Application for AiAssistant {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = (LangFlag, Cli);

    fn new(flags: (LangFlag, Cli)) -> (AiAssistant, Command<Message>) {
        println!("Application::new({:?})", flags);
        (
            AiAssistant {lang:flags.0, state:PokedexState::Loading, env:flags.1},
            Command::perform(Pokemon::test(), Message::PokemonFound),
        )
    }

    fn title(&self) -> String {
        let subtitle = match self {
            AiAssistant {state:PokedexState::Loading, ..} => "Loading",
            AiAssistant {state:PokedexState::Loaded { pokemon, .. }, ..}=> &pokemon.name,
            AiAssistant {state:PokedexState::Errored { .. }, ..} => "Whoops!",
        };

        format!("{subtitle} - Pokédex")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::PokemonFound(Ok(pokemon)) => {
                println!("update():PokemonFound(Ok())");
                self.state = PokedexState::Loaded { pokemon };

                Command::none()
            }
            Message::PokemonFound(Err(_error)) => {
                println!("update():PokemonFound(Err())");
                self.state = PokedexState::Errored;

                Command::none()
            }
            Message::Search => match self {
                AiAssistant {state:PokedexState::Loading, ..} => {
                    println!("update():Search Loading");
                    Command::none()
                },
                _ => {
                    println!("update():Search otherwise");
                    self.state = PokedexState::Loading;

                    Command::perform(Pokemon::search(self.lang.clone()), Message::PokemonFound)
                }
            },
            Message::Test => {
                println!("update():Test");
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let content = match &self.state {
            PokedexState::Loading => {
                column![text("Searching for Pokémon...").size(40),]
                    .width(Length::Shrink)
            }
            PokedexState::Loaded { pokemon } => column![
                pokemon.view(),
                button("Keep searching!").on_press(Message::Search)
            ]
            .max_width(500)
            .spacing(20)
            .align_items(Alignment::End),
            PokedexState::Errored => column![
                text("Whoops! Something went wrong...").size(40),
                button("Try again").on_press(Message::Search)
            ]
            .spacing(20)
            .align_items(Alignment::End),
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}

#[derive(Debug, Clone)]
struct Pokemon {
    number: u16,
    name: String,
    description: String,
    image: image::Handle,
}

impl Pokemon {
    const TOTAL: u16 = 807;

    fn view(&self) -> Element<Message> {
        row![
            image::viewer(self.image.clone()),
            column![
                row![
                    text(&self.name).size(30).width(Length::Fill),
                    text(format!("#{}", self.number))
                        .size(20)
                        .style(Color::from([0.5, 0.5, 0.5])),
                ]
                .align_items(Alignment::Center)
                .spacing(20),
                self.description.as_ref(),
            ]
            .spacing(20),
        ]
        .spacing(20)
        .align_items(Alignment::Center)
        .into()
    }

    async fn test() -> Result<Pokemon, Error> {
        Err(Error::APIError)
    }

    async fn search(lang: LangFlag) -> Result<Pokemon, Error> {
        use rand::Rng;
        use serde::Deserialize;

        #[derive(Debug, Deserialize)]
        struct Entry {
            name: String,
            flavor_text_entries: Vec<FlavorText>,
        }

        #[derive(Debug, Deserialize)]
        struct FlavorText {
            flavor_text: String,
            language: Language,
        }

        #[derive(Debug, Deserialize)]
        struct Language {
            name: String,
        }

        let id = {
            let mut rng = rand::rngs::OsRng;

            rng.gen_range(0..Pokemon::TOTAL)
        };

        let fetch_entry = async {
            let url = format!("https://pokeapi.co/api/v2/pokemon-species/{id}");

            reqwest::get(&url).await?.json().await
        };

        let (entry, image): (Entry, _) =
            futures::future::try_join(fetch_entry, Self::fetch_image(id))
                .await?;

        let lang_string :String = lang.clone().into();
        let description = entry
            .flavor_text_entries
            .iter()
            .find(|text| text.language.name == lang_string)
            .ok_or(Error::LanguageError)?;

        Ok(Pokemon {
            number: id,
            name: entry.name.to_uppercase(),
            description: description
                .flavor_text
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect(),
            image,
        })
    }

    async fn fetch_image(id: u16) -> Result<image::Handle, reqwest::Error> {
        let url = format!(
            "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon/{id}.png"
        );

        #[cfg(not(target_arch = "wasm32"))]
        {
            let bytes = reqwest::get(&url).await?.bytes().await?;

            Ok(image::Handle::from_memory(bytes))
        }

        #[cfg(target_arch = "wasm32")]
        Ok(image::Handle::from_path(url))
    }
}

#[derive(Debug, Clone)]
enum Error {
    APIError,
    LanguageError,
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Error {
        dbg!(error);

        Error::APIError
    }
}

fn button(text: &str) -> widget::Button<'_, Message> {
    widget::button(text).padding(1)
}
