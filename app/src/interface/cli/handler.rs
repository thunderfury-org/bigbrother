use crate::{
    application::import::{MetadataLookup, ParsedMediaInfo},
    application::recorded_import::RecordedImportService,
    error::{self, AppResult},
    infrastructure::share::resolver::ShareResolver,
};

use crate::interface::import::{
    NO_NEW_MEDIA_MESSAGE, format_import_summaries, format_verbose_import_notes,
    source_for_share_url,
};

use super::{context::CliContext, logger, telegram_export::TelegramExportIndexRuntimeRunner};

pub(crate) async fn run_import_share_url(
    data_dir: &str,
    url: &str,
    verbose: bool,
    description: Option<String>,
) -> AppResult<()> {
    if verbose {
        logger::init_console();
    }

    let ctx = CliContext::new(data_dir)?;
    let share_resolver = ctx.share_resolver();
    let mut import_service = ctx.import_service().await?;
    let mut metadata_lookup = MetadataLookup::default();
    let file_index_service = ctx.file_index_service().await?;
    let recorded = RecordedImportService::new(ctx.import_record_repository().await?);

    // Fetch raw files once
    let raw_files = resolve_share_url_raw_files(&share_resolver, url).await?;

    // Index: reuse raw files
    if !raw_files.is_empty()
        && let Err(err) = file_index_service
            .record_raw_files(raw_files.clone(), description.clone())
            .await
    {
        eprintln!("Warning: failed to index share url: {err}");
    }

    // Import: reuse raw files
    let descriptions: Vec<String> = description.into_iter().collect();
    let media_files = metadata_lookup.build_media_files(raw_files, descriptions);
    let imported = recorded
        .execute(source_for_share_url(url), || async {
            import_service.transfer_media_files(&media_files).await
        })
        .await?;
    let summaries = format_import_summaries(&imported);
    let verbose_notes = if verbose {
        format_verbose_import_notes(&imported)
    } else {
        Vec::new()
    };

    if summaries.is_empty() {
        println!("{NO_NEW_MEDIA_MESSAGE}");
    } else {
        for summary in summaries {
            println!("{summary}");
        }
    }

    for note in verbose_notes {
        println!("{note}");
    }

    if verbose && imported.is_empty() {
        println!(
            "详细信息: 本次没有生成任何导入结果，常见原因包括分享中没有可识别媒体、TMDB 未匹配到条目，或电影资源在入库前就被判定为已存在且无需覆盖。"
        );
    }

    Ok(())
}

pub(crate) async fn run_share_parse(
    data_dir: &str,
    url: &str,
    description: Option<String>,
) -> AppResult<()> {
    let ctx = CliContext::new(data_dir)?;
    let share_resolver = ctx.share_resolver();
    let mut parse_service = ctx.parse_service().await?;

    let raw_files = resolve_share_url_raw_files(&share_resolver, url).await?;
    let descriptions: Vec<String> = description.into_iter().collect();
    let results = parse_service
        .parse_media_files(raw_files, descriptions)
        .await?;

    if results.is_empty() {
        println!("未找到可解析的媒体文件");
        return Ok(());
    }

    for line in format_parse_results(&results) {
        println!("{line}");
    }

    Ok(())
}

pub(crate) async fn run_share_list(data_dir: &str, url: &str) -> AppResult<()> {
    let ctx = CliContext::new(data_dir)?;
    let share_resolver = ctx.share_resolver();
    let raw_files = resolve_share_url_raw_files(&share_resolver, url).await?;

    for line in format_share_list_output(&raw_files) {
        println!("{line}");
    }

    Ok(())
}

pub(crate) async fn run_search_files(data_dir: &str, keyword: &str, limit: u64) -> AppResult<()> {
    let ctx = CliContext::new(data_dir)?;
    let service = ctx.file_index_service().await?;
    let results = service.search_files(keyword, limit).await?;
    if results.is_empty() {
        println!("未找到匹配文件");
        return Ok(());
    }

    for (index, record) in results.iter().enumerate() {
        let display_name = record
            .locations
            .first()
            .map(|location| location.file_name.as_str())
            .unwrap_or("(unknown)");
        println!("{}. {}", index + 1, display_name);
        println!("   size: {}", format_file_size(record.size));
        println!("   {}: {}", record.hash_type, record.hash_value);
        for location in &record.locations {
            println!("   file: {}", location.file_name);
            println!("   path: {}", location.file_path);
            for description in location.descriptions.iter().take(3) {
                println!("   description: {description}");
            }
        }
    }

    Ok(())
}

pub(crate) async fn run_telegram_export_index(
    data_dir: &str,
    input: &str,
    verbose: bool,
    delay_ms: u64,
    retry_all: bool,
) -> AppResult<()> {
    if verbose {
        logger::init_console();
    }

    let ctx = CliContext::new(data_dir)?;
    let (share_resolver, file_index_repo, state_repo) =
        ctx.telegram_export_index_services().await?;
    let runner = TelegramExportIndexRuntimeRunner::new(share_resolver, file_index_repo, state_repo);
    runner.run(input, delay_ms, retry_all).await
}

fn format_file_size(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = size as f64;
    let mut unit_index = 0;

    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{size} B")
    } else {
        format!("{value:.2} {} ({size} bytes)", UNITS[unit_index])
    }
}

fn format_share_list_output(raw_files: &[crate::domain::share::RawFile]) -> Vec<String> {
    if raw_files.is_empty() {
        return vec!["未找到任何文件".to_owned()];
    }

    let mut lines = Vec::new();
    let mut total_size = 0_u64;

    for (index, file) in raw_files.iter().enumerate() {
        total_size += file.size;
        lines.push(format!("{}. {}", index + 1, file.name));
        lines.push(format!("   path: {}", display_share_path(&file.path)));
        lines.push(format!("   size: {}", format_file_size(file.size)));
        match &file.hash {
            crate::domain::share::FileHash::Md5(value) => lines.push(format!("   md5: {value}")),
            crate::domain::share::FileHash::Sha1(value) => lines.push(format!("   sha1: {value}")),
        }
        if index + 1 < raw_files.len() {
            lines.push(String::new());
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "共 {} 个文件，总大小 {}",
        raw_files.len(),
        format_file_size(total_size)
    ));

    lines
}

fn format_parse_results(results: &[ParsedMediaInfo]) -> Vec<String> {
    let mut lines = Vec::new();

    for (index, info) in results.iter().enumerate() {
        match info {
            ParsedMediaInfo::Movie {
                file_name,
                path,
                titles,
                year,
                resolution,
                quality,
                video_codec,
                audio_codec,
                hdr,
                tmdb_id,
                tmdb_title,
                tmdb_year,
                subtitle_files,
            } => {
                lines.push(format!("{}. {}", index + 1, file_name));
                lines.push(format!("   路径: {}", display_share_path(path)));
                if !titles.is_empty() {
                    lines.push(format!("   标题: {}", titles.join(" / ")));
                }
                if !year.is_empty() {
                    lines.push(format!("   年份: {year}"));
                }
                let tech = format_tech_info(resolution, quality, video_codec, audio_codec, hdr);
                if !tech.is_empty() {
                    lines.push(format!("   {tech}"));
                }
                lines.push(format_tmdb(tmdb_id, tmdb_title, tmdb_year));
                if !subtitle_files.is_empty() {
                    lines.push(format!("   字幕: {}", subtitle_files.join(", ")));
                }
            }
            ParsedMediaInfo::Tv {
                file_name,
                path,
                titles,
                year,
                season_number,
                episode_number,
                second_episode_number,
                resolution,
                quality,
                video_codec,
                audio_codec,
                hdr,
                tmdb_id,
                tmdb_title,
                tmdb_year,
                subtitle_files,
            } => {
                lines.push(format!("{}. {}", index + 1, file_name));
                lines.push(format!("   路径: {}", display_share_path(path)));
                if !titles.is_empty() {
                    lines.push(format!("   标题: {}", titles.join(" / ")));
                }
                if !year.is_empty() {
                    lines.push(format!("   年份: {year}"));
                }
                if let Some(ep) =
                    format_episode(season_number, episode_number, second_episode_number)
                {
                    lines.push(format!("   集数: {ep}"));
                }
                let tech = format_tech_info(resolution, quality, video_codec, audio_codec, hdr);
                if !tech.is_empty() {
                    lines.push(format!("   {tech}"));
                }
                lines.push(format_tmdb(tmdb_id, tmdb_title, tmdb_year));
                if !subtitle_files.is_empty() {
                    lines.push(format!("   字幕: {}", subtitle_files.join(", ")));
                }
            }
            ParsedMediaInfo::Unmatched {
                file_name,
                path,
                titles,
                year,
            } => {
                lines.push(format!("{}. {}", index + 1, file_name));
                lines.push(format!("   路径: {}", display_share_path(path)));
                if !titles.is_empty() {
                    lines.push(format!("   标题: {}", titles.join(" / ")));
                }
                if !year.is_empty() {
                    lines.push(format!("   年份: {year}"));
                }
                lines.push("   TMDB: 未匹配".to_owned());
            }
        }

        if index + 1 < results.len() {
            lines.push(String::new());
        }
    }

    lines
}

fn format_tech_info(
    resolution: &str,
    quality: &str,
    video_codec: &str,
    audio_codec: &str,
    hdr: &str,
) -> String {
    let mut parts = Vec::new();
    if !resolution.is_empty() {
        parts.push(format!("分辨率: {resolution}"));
    }
    if !quality.is_empty() {
        parts.push(format!("质量: {quality}"));
    }
    if !video_codec.is_empty() {
        let codec = if !audio_codec.is_empty() {
            format!("{video_codec} / {audio_codec}")
        } else {
            video_codec.to_string()
        };
        parts.push(format!("编码: {codec}"));
    } else if !audio_codec.is_empty() {
        parts.push(format!("编码: {audio_codec}"));
    }
    if !hdr.is_empty() {
        parts.push(format!("HDR: {hdr}"));
    }
    parts.join(" | ")
}

fn format_episode(
    season: &Option<u32>,
    episode: &Option<u32>,
    second_episode: &Option<u32>,
) -> Option<String> {
    match (season, episode) {
        (Some(s), Some(e)) => {
            let ep = if let Some(e2) = second_episode {
                format!("S{s:02}E{e:02}-E{e2:02}")
            } else {
                format!("S{s:02}E{e:02}")
            };
            Some(ep)
        }
        (None, Some(e)) => Some(format!("E{e:02}")),
        _ => None,
    }
}

fn format_tmdb(id: &Option<u32>, title: &Option<String>, year: &Option<String>) -> String {
    match (id, title, year) {
        (Some(id), Some(title), Some(year)) => format!("   TMDB: {title} ({year}) [ID: {id}]"),
        (Some(id), Some(title), None) => format!("   TMDB: {title} [ID: {id}]"),
        _ => "   TMDB: 未匹配".to_owned(),
    }
}

fn display_share_path(path: &str) -> &str {
    if path.is_empty() { "/" } else { path }
}

async fn resolve_share_url_raw_files<R: ShareResolver>(
    resolver: &R,
    url: &str,
) -> AppResult<Vec<crate::domain::share::RawFile>> {
    resolver.raw_files_from_url(url).await?.ok_or_else(|| {
        error::AppError::InvalidParameter(format!(
            "unsupported share url '{url}', expected pan123, pan189, or pan115 share link"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{format_file_size, format_share_list_output, resolve_share_url_raw_files};
    use crate::{
        domain::share::{FileHash, RawFile},
        error::AppResult,
        infrastructure::share::resolver::ShareResolver,
    };

    #[derive(Clone)]
    struct FakeShareResolver {
        result: Option<Vec<RawFile>>,
    }

    impl ShareResolver for FakeShareResolver {
        async fn raw_files_from_url(&self, _url: &str) -> AppResult<Option<Vec<RawFile>>> {
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn resolve_share_url_raw_files_rejects_unsupported_provider() {
        let err = resolve_share_url_raw_files(
            &FakeShareResolver { result: None },
            "https://example.com/s/test",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, crate::error::AppError::InvalidParameter(_)));
        assert!(err.to_string().contains("unsupported share url"));
    }

    #[test]
    fn format_file_size_keeps_bytes_and_adds_readable_unit() {
        assert_eq!(
            format_file_size(6_517_230_688),
            "6.07 GiB (6517230688 bytes)"
        );
        assert_eq!(format_file_size(512), "512 B");
    }

    #[test]
    fn format_share_list_output_uses_expected_text_layout() {
        let output = format_share_list_output(&[
            RawFile {
                id: Some(1),
                name: "Movie.mkv".into(),
                hash: FileHash::Md5("abcdef0123456789abcdef0123456789".into()),
                size: 6_517_230_688,
                path: String::new(),
            },
            RawFile {
                id: None,
                name: "Episode 01.mkv".into(),
                hash: FileHash::Sha1("abcdef0123456789abcdef0123456789abcdef01".into()),
                size: 512,
                path: "/Show/Season 01".into(),
            },
        ]);

        assert_eq!(
            output,
            vec![
                "1. Movie.mkv",
                "   path: /",
                "   size: 6.07 GiB (6517230688 bytes)",
                "   md5: abcdef0123456789abcdef0123456789",
                "",
                "2. Episode 01.mkv",
                "   path: /Show/Season 01",
                "   size: 512 B",
                "   sha1: abcdef0123456789abcdef0123456789abcdef01",
                "",
                "共 2 个文件，总大小 6.07 GiB (6517231200 bytes)",
            ]
        );
    }

    #[test]
    fn format_share_list_output_reports_empty_result() {
        assert_eq!(format_share_list_output(&[]), vec!["未找到任何文件"]);
    }
}
