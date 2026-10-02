use super::Lang;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Phone {
    ReadTheText,
    OcrWhy,
    OcrAbout,
    DownloadsOnce(u64),
    ReadingPage { page: usize, of: usize },
    SecondsIn { seconds: u64 },
    BytesOf { done: u64, of: u64 },
    Language,
    AddALanguage,
    DoneAdding,
    SearchLanguages,
    OnThisPhone,
    TickOnlyWhatIsThere,
    Pages,
    EveryPage,
}

impl Phone {
    #[must_use]
    pub fn say(&self, lang: Lang) -> String {
        match lang {
            Lang::English => self.english(),
        }
    }

    fn english(&self) -> String {
        match self {
            Self::ReadTheText => "Read the text".to_owned(),
            Self::OcrWhy => {
                "Reads scanned pages on this phone with Tesseract, a small open-source \
                model: it can misread, so check the text. The languages you tick are downloaded \
                once and kept on the phone; nothing else is sent anywhere."
                    .to_owned()
            }
            Self::OcrAbout => "Turns a scanned page into text you can search and copy. It runs on \
                this phone and can misread, so check what it finds."
                .to_owned(),
            Self::DownloadsOnce(bytes) => {
                format!(
                    "Downloads {} once, then works offline",
                    super::megabytes(*bytes)
                )
            }
            Self::ReadingPage { page, of } => format!("Reading page {page} of {of}"),
            Self::SecondsIn { seconds } => {
                format!("{seconds} s \u{00b7} a page takes up to a minute")
            }
            Self::BytesOf { done, of } => {
                format!("{} of {}", super::megabytes(*done), super::megabytes(*of))
            }
            Self::Language => "Language".to_owned(),
            Self::AddALanguage => "+  Add a language".to_owned(),
            Self::DoneAdding => "Done".to_owned(),
            Self::SearchLanguages => "Search languages".to_owned(),
            Self::OnThisPhone => "on this phone".to_owned(),
            Self::TickOnlyWhatIsThere => "Tick only the languages in the document: one it does \
                not have makes reading slower and can make it worse."
                .to_owned(),
            Self::Pages => "Pages".to_owned(),
            Self::EveryPage => "Every page".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Lang, Phone};

    #[test]
    fn every_sentence_for_the_phone_says_something_and_names_what_it_is_given() {
        let all = [
            Phone::ReadTheText,
            Phone::OcrWhy,
            Phone::OcrAbout,
            Phone::DownloadsOnce(2_652_786),
            Phone::ReadingPage { page: 2, of: 5 },
            Phone::SecondsIn { seconds: 14 },
            Phone::BytesOf {
                done: 1_048_576,
                of: 2_097_152,
            },
            Phone::Language,
            Phone::AddALanguage,
            Phone::DoneAdding,
            Phone::SearchLanguages,
            Phone::OnThisPhone,
            Phone::TickOnlyWhatIsThere,
            Phone::Pages,
            Phone::EveryPage,
        ];
        for sentence in &all {
            assert!(
                !sentence.say(Lang::English).trim().is_empty(),
                "{sentence:?}"
            );
        }
        assert_eq!(
            Phone::ReadingPage { page: 2, of: 5 }.say(Lang::English),
            "Reading page 2 of 5"
        );
        assert_eq!(
            Phone::DownloadsOnce(2_652_786).say(Lang::English),
            "Downloads 2.5 MB once, then works offline"
        );
        assert_eq!(
            Phone::BytesOf {
                done: 1_048_576,
                of: 2_097_152
            }
            .say(Lang::English),
            "1.0 MB of 2.0 MB"
        );
    }
}
