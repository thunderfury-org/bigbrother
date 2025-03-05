use std::{collections::HashMap, path::Path};

use regex::Regex;
use tracing::info;

use crate::{
    common::{
        config::TaskConfig,
        error::{Error, Result},
        state::AppState,
    },
    parser::{EpisodeInfo, FileType},
};

use super::{
    alist::{self, File},
    push, tmdb,
};

#[derive(Debug)]
struct TvInfo {
    name: String,
    year: String,
    number_of_seasons: i32,
}

struct EpisodeFile {
    file_dir: String,
    file_path: String,
    file_name: String,
    extension: String,
    // size: u64,
}

pub struct TvProcessor<'a> {
    state: &'a AppState,
    task: &'a TaskConfig,
    tmdb_client: tmdb::Client<'a>,
    alist_client: alist::Client<'a>,
}

impl TvProcessor<'_> {
    pub fn new<'a>(state: &'a AppState, task: &'a TaskConfig) -> Result<TvProcessor<'a>> {
        Ok(TvProcessor {
            state,
            task,
            tmdb_client: tmdb::Client::try_from(state)?,
            alist_client: alist::Client::try_from(state)?,
        })
    }

    pub async fn run(&self) -> Result<()> {
        let files = self.alist_client.list(&self.task.src_dir).await?;

        for file in &files {
            if !file.is_dir {
                continue;
            }
            info!("processing dir: {:?}", file);

            let (name, year) = TvProcessor::parse_tv_name(&file.name);
            self.process_one_tv(&name, year, &file.path).await?;
        }

        Ok(())
    }

    /// Parses the TV show name and year from a string formatted like "xxx (2022)".
    ///
    /// # Arguments
    ///
    /// * `name` - A string slice that holds the name and year of the TV show.
    ///
    /// # Returns
    ///
    /// A tuple containing the TV show name and an optional year as an integer.
    fn parse_tv_name(name: &str) -> (String, Option<i32>) {
        // Regular expression to capture year in parentheses
        let re = Regex::new(r"([^\(（]+)\s*([\(（]\s*(\d{4})\s*[\)）])?").unwrap();

        // Attempt to capture the year from the input name
        if let Some(caps) = re.captures(name) {
            let name = caps.get(1).unwrap().as_str();
            let year = caps.get(3).map(|y| y.as_str().parse().unwrap());
            return (name.trim().to_string(), year);
        }

        // Return the name as is with None for the year if parsing fails
        (name.trim().to_string(), None)
    }

    fn parse_season_number(&self, name: &str) -> Option<i32> {
        let re = Regex::new(r"(?i)S(easons?)?\s*(\d{1,3})").unwrap();
        re.captures(name)
            .map(|caps| caps.get(2).unwrap().as_str().parse::<i32>().unwrap())
    }

    async fn process_one_tv(&self, name: &str, year: Option<i32>, path: &str) -> Result<()> {
        let files: Vec<File> = self.alist_client.list(path).await?;
        if files.is_empty() {
            return Ok(());
        }

        let tv_info = self.get_tv_info(name, year).await?;
        info!("found tv info: {:?}", tv_info);

        self.process_one_folder(
            path,
            &tv_info,
            if tv_info.number_of_seasons == 1 { Some(1) } else { None },
        )
        .await?;

        for file in &files {
            if !file.is_dir {
                continue;
            }
            let season_number = self.parse_season_number(&file.name);
            self.process_one_folder(&file.path, &tv_info, season_number).await?;
        }

        Ok(())
    }

    async fn process_one_folder(&self, path: &str, tv_info: &TvInfo, season_number: Option<i32>) -> Result<()> {
        let episode_files: Vec<File> = self
            .alist_client
            .list(path)
            .await?
            .into_iter()
            .filter(|f| !f.is_dir)
            .collect();
        if episode_files.is_empty() {
            return Ok(());
        }

        let episodes = self.parse_episodes(&episode_files, season_number);

        for (season_number, _episode_map) in episodes {
            let dest_path = format!(
                "{}/{} ({})/Season {:02}",
                self.task.dest_dir, tv_info.name, tv_info.year, season_number
            );

            self.process_one_season(&tv_info.name, season_number, &_episode_map, dest_path.as_str())
                .await?;
        }

        Ok(())
    }

    async fn process_one_season(
        &self,
        tv_name: &str,
        season_number: i32,
        episode_map: &HashMap<i32, EpisodeFile>,
        dest_path: &str,
    ) -> Result<()> {
        self.alist_client.mkdir(dest_path).await?;

        let exist_files: Vec<File> = self
            .alist_client
            .list(dest_path)
            .await?
            .into_iter()
            .filter(|f| !f.is_dir)
            .collect();

        let exists_seasons = self.parse_episodes(&exist_files, None);
        let empty_map = HashMap::new();
        let exist_episodes = exists_seasons.get(&season_number).unwrap_or(&empty_map);

        let mut moved_episodes: Vec<i32> = vec![];
        for (episode_number, file) in episode_map {
            if exist_episodes.contains_key(episode_number) {
                continue;
            }

            let dest_file_name = format!(
                "{}.S{:02}E{:02}.{}",
                tv_name, season_number, episode_number, file.extension
            );
            if dest_file_name != file.file_name {
                info!(
                    "rename file: {} -> {}/{}",
                    file.file_path, file.file_dir, dest_file_name
                );
                self.alist_client
                    .rename(file.file_path.as_str(), dest_file_name.as_str())
                    .await?;
            }

            info!("move file: {}/{} -> {}", file.file_dir, dest_file_name, dest_path);
            // move file
            self.alist_client
                .move_file(&file.file_dir, dest_path, &dest_file_name)
                .await?;

            moved_episodes.push(*episode_number);
        }

        if !moved_episodes.is_empty() {
            moved_episodes.sort();
            push::send(
                self.state,
                self.format_message(tv_name, season_number, &moved_episodes).as_str(),
            )
            .await;
        }

        Ok(())
    }

    fn format_message(&self, tv_name: &str, season_number: i32, episodes: &[i32]) -> String {
        let first = episodes.first().unwrap();
        let last = episodes.last().unwrap();
        if first == last {
            format!("{} 第 {} 季第 {} 集已就绪", tv_name, season_number, first)
        } else {
            format!("{} 第 {} 季 {} - {} 集已就绪", tv_name, season_number, first, last)
        }
    }

    async fn get_tv_info(&self, title: &str, year: Option<i32>) -> Result<TvInfo> {
        let results = self.tmdb_client.search_tv(title, year).await?;
        if results.is_empty() {
            return Err(Error::NotFound(format!("no tv found in tmdb for {}", title)));
        }

        let detail = self.tmdb_client.get_tv_detail(results[0].id).await?;

        Ok(TvInfo {
            name: detail.name.clone(),
            year: detail.first_air_date.split("-").next().unwrap().to_string(),
            number_of_seasons: detail.number_of_seasons,
        })
    }

    fn parse_episodes(
        &self,
        files: &Vec<File>,
        default_season: Option<i32>,
    ) -> HashMap<i32, HashMap<i32, EpisodeFile>> {
        let mut result: HashMap<i32, HashMap<i32, EpisodeFile>> = HashMap::new();

        for file in files {
            if file.is_dir {
                continue;
            }

            let info = EpisodeInfo::from(file.name.as_str());
            if info.file_type != FileType::Video {
                continue;
            }
            if info.episode_number.is_none() {
                info!("can not find episode number from file {}", file.name);
                continue;
            }

            let season_number = match info.season_number {
                Some(n) => n,
                None => match default_season {
                    Some(s) => s,
                    None => {
                        info!("can not find season number from file {}", file.name);
                        continue;
                    }
                },
            };

            result.entry(season_number).or_default().insert(
                info.episode_number.unwrap(),
                EpisodeFile {
                    file_dir: Path::new(file.path.as_str())
                        .parent()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string(),
                    file_path: file.path.clone(),
                    file_name: file.name.clone(),
                    extension: info.extension.unwrap(),
                    // size: file.size,
                },
            );
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::TvProcessor;

    #[test]
    fn test_parse_tv_name_valid_with_year() {
        let input = "The Office (2005)";
        let expected = ("The Office".to_string(), Some(2005));
        assert_eq!(TvProcessor::parse_tv_name(input), expected);
    }

    #[test]
    fn test_parse_tv_name_valid_without_year() {
        let input = "The Office";
        let expected = ("The Office".to_string(), None);
        assert_eq!(TvProcessor::parse_tv_name(input), expected);
    }

    #[test]
    fn test_parse_tv_name_invalid_format() {
        let input = "The Office 2005";
        let expected = ("The Office 2005".to_string(), None);
        assert_eq!(TvProcessor::parse_tv_name(input), expected);
    }

    #[test]
    fn test_parse_tv_name_multiple_spaces() {
        let input = "The  Office  (2005)";
        let expected = ("The  Office".to_string(), Some(2005));
        assert_eq!(TvProcessor::parse_tv_name(input), expected);
    }

    #[test]
    fn test_parse_tv_name_special_characters() {
        let input = "The Office: US (2005)";
        let expected = ("The Office: US".to_string(), Some(2005));
        assert_eq!(TvProcessor::parse_tv_name(input), expected);
    }

    #[test]
    fn test_parse_tv_name_zh_characters() {
        let input = "The Office: US （2005）";
        let expected = ("The Office: US".to_string(), Some(2005));
        assert_eq!(TvProcessor::parse_tv_name(input), expected);
    }
}
