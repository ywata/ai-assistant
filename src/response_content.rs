#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Marker { text: String, lang: Option<String> },
    Content { text: String, lang: Option<String> },
}

pub fn split_code(source: &str, markers: &Vec<regex::Regex>) -> Vec<Mark> {
    let mut curr_pos: usize = 0; // index to source
    let max = source.len();
    let mut result = Vec::new();
    let mut lang = None;
    for marker in markers {
        // source is searched starting from curr_pos to max
        if let Some(matched) = marker.captures(&source[curr_pos..max]) {
            let all_matched = matched.get(0).unwrap();

            let pos = all_matched.range().start; // position in [curr..pos.. <max]
            if 0 != pos {
                // As there is some text before marker, it becomes Content
                result.push(Mark::Content {
                    text: String::from(&source[curr_pos..(curr_pos + pos)]),
                    lang: lang.clone(),
                });
                curr_pos += pos;
            }
            lang = matched.get(1).map(|m| {
                String::from(
                    &source[curr_pos + m.range().start - pos..(curr_pos + m.range().end - pos)],
                )
            });
            let r = all_matched.range();
            let len = r.end - r.start;

            result.push(Mark::Marker {
                text: String::from(&source[curr_pos..curr_pos + len]),
                lang: lang.clone(),
            });
            curr_pos += len;
        } else {
            // not marker found. This might be a error.
        }
    }
    if curr_pos < max {
        result.push(Mark::Content {
            text: String::from(&source[curr_pos..max]),
            lang: lang.clone(),
        });
    }

    result
}

pub fn get_content(contents: Vec<Mark>) -> Option<Mark> {
    let mut res = None;
    for c in contents {
        if let Mark::Content { .. } = c {
            res = Some(c);
            break;
        }
    }
    res
}

mod test {
    use super::*;
    use regex::Regex;
    #[test]
    fn test_split_mark_only() {
        let input = r#"```start
```"#
            .to_string();
        let markers = vec!["```(start)".to_string(), "```".to_string()];
        let rex_markers: Result<Vec<_>, _> = markers.iter().map(|s| Regex::new(s)).collect();

        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 3);
        assert_eq!(
            res.first(),
            Some(&Mark::Marker {
                text: "```start".to_string(),
                lang: Some("start".to_string())
            })
        );
        assert_eq!(
            res.get(1),
            Some(&Mark::Content {
                text: "\n".to_string(),
                lang: Some("start".to_string())
            })
        );
        assert_eq!(
            res.get(2),
            Some(&Mark::Marker {
                text: "```".to_string(),
                lang: None
            })
        );
    }
    #[test]
    fn test_split_mark_backquotes() {
        let input = r#"```start
asdf
```"#
            .to_string();
        let markers = vec!["```start```".to_string()];
        let rex_markers: Result<Vec<_>, _> = markers.iter().map(|s| Regex::new(s)).collect();

        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 1);
        assert_eq!(
            res.first(),
            Some(&Mark::Content {
                text: "```start\nasdf\n```".to_string(),
                lang: None
            })
        );
    }

    #[test]
    fn test_split_mark_and_content() {
        let input = r#"asdf
```start
hjklm
```
xyzw
"#
        .to_string();
        let markers = vec!["```(start)".to_string(), "```".to_string()];
        let rex_markers: Result<Vec<_>, _> = markers.iter().map(|s| Regex::new(s)).collect();

        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 5);
        assert_eq!(
            res.first(),
            Some(&Mark::Content {
                text: "asdf\n".to_string(),
                lang: None
            })
        );
        assert_eq!(
            res.get(1),
            Some(&Mark::Marker {
                text: "```start".to_string(),
                lang: Some("start".to_string())
            })
        );
        //assert_eq!(res.get(2), Some(&Mark::Content{text:"\nhjklm\n".to_string(), lang: Some("start".to_string())}));
        //assert_eq!(res.get(3), Some(&Mark::Marker{text:"```".to_string(), lang: None}));
        //assert_eq!(res.get(4), Some(&Mark::Content{text:"\nxyzw\n".to_string(), lang: None}));
    }
    #[test]
    fn test_regex() {
        let rex_str = r#"^([a-zA-Z]+)[0-9]+"#;
        let rex = Regex::new(rex_str).unwrap();

        let input = r#"abcd123x"#;

        if let Some(m1) = rex.captures(input) {
            let g0 = m1.get(0).unwrap().as_str();
            let g1 = m1.get(1).unwrap().as_str();
            assert_eq!(g0, "abcd123");
            assert_eq!(g1, "abcd");
            return;
        }
        assert_eq!(true, false);
    }
}
