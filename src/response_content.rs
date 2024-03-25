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
