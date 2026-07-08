<<<<<<< HEAD
=======
use audiocontrol::helpers::artist_splitter::ARTIST_SPLIT_CACHE_PREFIX;
use audiocontrol::helpers::attribute_cache::{self, AttributeCache};
use audiocontrol::helpers::image_meta::IMAGE_META_CACHE_PREFIX;
use audiocontrol::helpers::musicbrainz::{ARTIST_MBID_CACHE_PREFIX, ARTIST_NOT_FOUND_CACHE_PREFIX};
use chrono::DateTime;
>>>>>>> origin/main
use clap::{Parser, Subcommand};
use log::{info, warn};
use audiocontrol::helpers::attribute_cache::{self, AttributeCache};
use audiocontrol::helpers::artist_splitter::ARTIST_SPLIT_CACHE_PREFIX;
use audiocontrol::helpers::musicbrainz::{ARTIST_MBID_CACHE_PREFIX, ARTIST_NOT_FOUND_CACHE_PREFIX};
use audiocontrol::helpers::image_meta::IMAGE_META_CACHE_PREFIX;
use std::path::PathBuf;
use chrono::DateTime;

#[derive(Parser)]
#[command(name = "audiocontrol_dump_cache")]
#[command(about = "A tool to manage AudioControl cache database")]
#[command(long_about = None)]
struct Cli {
    /// Path to the cache database directory
    #[arg(short, long, value_name = "DIR")]
    cache_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List cache entries
    List {
        /// Filter entries by prefix (e.g., "artist::", "theaudiodb::")
        #[arg(short, long)]
        prefix: Option<String>,
        
        /// Show detailed information including size and timestamps
        #[arg(short, long)]
        detailed: bool,

        /// Limit number of results
        #[arg(short, long)]
        limit: Option<usize>,

        /// Show artist MusicBrainz data (shortcut for --prefix "artist::mbid")
        #[arg(long)]
        artistmbid: bool,

        /// Show image metadata (shortcut for --prefix "image_meta:")
        #[arg(long)]
        imagemeta: bool,

        /// Show artist split cache (shortcut for --prefix "artist::split")
        #[arg(long)]
        artistsplit: bool,

        /// Show MusicBrainz negative cache (shortcut for --prefix "artist::not_found")
        #[arg(long)]
        artistnotfound: bool,
    },
    /// Clean cache entries
    Clean {
        /// Remove entries matching this prefix
        #[arg(short, long)]
        prefix: Option<String>,
        
        /// Remove all entries (use with caution!)
        #[arg(long)]
        all: bool,

        /// Remove entries older than specified days
        #[arg(long)]
        older_than_days: Option<u64>,

        /// Dry run - show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,

        /// Clean artist MusicBrainz data (shortcut for --prefix "artist::mbid")
        #[arg(long)]
        artistmbid: bool,

        /// Clean image metadata (shortcut for --prefix "image_meta:")
        #[arg(long)]
        imagemeta: bool,

        /// Clean artist split cache (shortcut for --prefix "artist::split")
        #[arg(long)]
        artistsplit: bool,

        /// Clean MusicBrainz negative cache (shortcut for --prefix "artist::not_found")
        #[arg(long)]
        artistnotfound: bool,
    },
    /// Show cache statistics
    Stats {
        /// Group statistics by prefix
        #[arg(short, long)]
        by_prefix: bool,
    },
}

fn determine_prefix(prefix: Option<&str>, artistmbid: bool, imagemeta: bool, artistsplit: bool, artistnotfound: bool) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let shortcut_count = [artistmbid, imagemeta, artistsplit, artistnotfound].iter().filter(|&&x| x).count();
    
    if shortcut_count > 1 {
        return Err("Cannot specify multiple shortcut options (--artistmbid, --imagemeta, --artistsplit, --artistnotfound) at once".into());
    }
    
    if prefix.is_some() && shortcut_count > 0 {
        return Err("Cannot specify both --prefix and shortcut options (--artistmbid, --imagemeta, --artistsplit, --artistnotfound)".into());
    }
    
    if artistmbid {
        Ok(Some(ARTIST_MBID_CACHE_PREFIX.trim_end_matches("::").to_string()))
    } else if imagemeta {
        Ok(Some(IMAGE_META_CACHE_PREFIX.trim_end_matches("::").to_string()))
    } else if artistsplit {
        Ok(Some(ARTIST_SPLIT_CACHE_PREFIX.trim_end_matches("::").to_string()))
    } else if artistnotfound {
        Ok(Some(ARTIST_NOT_FOUND_CACHE_PREFIX.trim_end_matches("::").to_string()))
    } else {
        Ok(prefix.map(|s| s.to_string()))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    // Initialize the cache with custom directory if provided
    if let Some(cache_dir) = cli.cache_dir {
        info!("Using cache directory: {}", cache_dir.display());
        AttributeCache::initialize_global(&cache_dir)?;
    } else {
        info!("Using default cache directory");
    }

    match &cli.command {
        Commands::List { prefix, detailed, limit, artistmbid, imagemeta, artistsplit, artistnotfound } => {
            let effective_prefix = determine_prefix(prefix.as_deref(), *artistmbid, *imagemeta, *artistsplit, *artistnotfound)?;
            list_cache_entries(effective_prefix.as_deref(), *detailed, *limit)?;
        }
        Commands::Clean { prefix, all, older_than_days, dry_run, artistmbid, imagemeta, artistsplit, artistnotfound } => {
            let effective_prefix = determine_prefix(prefix.as_deref(), *artistmbid, *imagemeta, *artistsplit, *artistnotfound)?;
            clean_cache_entries(effective_prefix.as_deref(), *all, *older_than_days, *dry_run)?;
        }
        Commands::Stats { by_prefix } => {
            show_cache_stats(*by_prefix)?;
        }
    }

    Ok(())
}

fn list_cache_entries(prefix: Option<&str>, detailed: bool, limit: Option<usize>) -> Result<(), Box<dyn std::error::Error>> {
    if detailed {
        let entries = attribute_cache::list_entries(prefix)?;
        let entries_to_show = if let Some(limit) = limit {
            &entries[..entries.len().min(limit)]
        } else {
            &entries
        };

        if entries_to_show.is_empty() {
            info!("No cache entries found{}", 
                  prefix.map(|p| format!(" with prefix '{}'", p)).unwrap_or_default());
            return Ok(());
        }

        println!("Cache Entries ({}{})", 
                 entries_to_show.len(),
                 if entries.len() > entries_to_show.len() { 
                     format!(" of {} total", entries.len()) 
                 } else { 
                     String::new() 
                 });
        println!("{:-<120}", "");
        println!("{:<60} {:>10} {:>20} {:>20}", "Key", "Size", "Created", "Updated");
        println!("{:-<120}", "");

        for entry in entries_to_show {
            let created = DateTime::from_timestamp(entry.created_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            
            let updated = DateTime::from_timestamp(entry.updated_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            let size_str = format_size(entry.size_bytes);
            
            println!("{:<60} {:>10} {:>20} {:>20}", 
                     truncate_key(&entry.key, 60), 
                     size_str, 
                     created, 
                     updated);
        }
    } else {
        let keys = attribute_cache::list_keys(prefix)?;
        let keys_to_show = if let Some(limit) = limit {
            &keys[..keys.len().min(limit)]
        } else {
            &keys
        };

        if keys_to_show.is_empty() {
            info!("No cache entries found{}", 
                  prefix.map(|p| format!(" with prefix '{}'", p)).unwrap_or_default());
            return Ok(());
        }

        println!("Cache Keys ({}{})", 
                 keys_to_show.len(),
                 if keys.len() > keys_to_show.len() { 
                     format!(" of {} total", keys.len()) 
                 } else { 
                     String::new() 
                 });
        println!("{:-<80}", "");

        for key in keys_to_show {
            println!("{}", key);
        }
    }

    Ok(())
}

fn clean_cache_entries(prefix: Option<&str>, all: bool, older_than_days: Option<u64>, dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    if all && prefix.is_some() {
        return Err("Cannot specify both --all and --prefix options".into());
    }

    if !all && prefix.is_none() && older_than_days.is_none() {
        return Err("Must specify either --all, --prefix, or --older-than-days".into());
    }

    if all {
        if dry_run {
            let entries = attribute_cache::list_entries(None)?;
            println!("Would delete {} cache entries (dry run)", entries.len());
            return Ok(());
        }

        warn!("Clearing ALL cache entries!");
        attribute_cache::clear()?;
        info!("All cache entries cleared");
        return Ok(());
    }

    if let Some(prefix) = prefix {
        if dry_run {
            let entries = attribute_cache::list_entries(Some(prefix))?;
<<<<<<< HEAD
            println!("Would delete {} cache entries with prefix '{}' (dry run)", entries.len(), prefix);
=======
            println!(
                "Would delete {} cache entries with prefix '{}' (dry run)",
                entries.len(),
                prefix
            );
>>>>>>> origin/main
            for entry in &entries[..entries.len().min(10)] {
                println!("  - {}", entry.key);
            }
            if entries.len() > 10 {
                println!("  ... and {} more", entries.len() - 10);
            }
            return Ok(());
        }

        let deleted = attribute_cache::remove_by_prefix(prefix)?;
        info!("Deleted {} cache entries with prefix '{}'", deleted, prefix);
        return Ok(());
    }

    if let Some(days) = older_than_days {
        // For now, we'll use the cleanup function which removes entries older than the configured max age
        // In the future, we could add a custom cleanup function that takes days as parameter
<<<<<<< HEAD
        warn!("Cleaning entries older than {} days using cache cleanup function", days);
=======
        warn!(
            "Cleaning entries older than {} days using cache cleanup function",
            days
        );
>>>>>>> origin/main
        let deleted = attribute_cache::cleanup()?;
        info!("Deleted {} old cache entries", deleted);
        return Ok(());
    }

    Ok(())
}

fn show_cache_stats(by_prefix: bool) -> Result<(), Box<dyn std::error::Error>> {
    let entries = attribute_cache::list_entries(None)?;
<<<<<<< HEAD
    
=======

>>>>>>> origin/main
    if entries.is_empty() {
        info!("Cache is empty");
        return Ok(());
    }

    let total_size: usize = entries.iter().map(|e| e.size_bytes).sum();
    let total_count = entries.len();

    println!("Cache Statistics");
    println!("{:-<50}", "");
    println!("Total entries: {}", total_count);
    println!("Total size: {}", format_size(total_size));
    
    if let (Some(oldest), Some(newest)) = (
        entries.iter().min_by_key(|e| e.created_at),
        entries.iter().max_by_key(|e| e.created_at)
    ) {
        let oldest_date = DateTime::from_timestamp(oldest.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        let newest_date = DateTime::from_timestamp(newest.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        
        println!("Oldest entry: {}", oldest_date);
        println!("Newest entry: {}", newest_date);
    }

    if by_prefix {
        let mut prefix_stats = std::collections::HashMap::new();
        
        for entry in &entries {
            let prefix = extract_prefix(&entry.key);
            let stats = prefix_stats.entry(prefix).or_insert((0, 0));
            stats.0 += 1;
            stats.1 += entry.size_bytes;
        }

        let mut sorted_prefixes: Vec<_> = prefix_stats.iter().collect();
        sorted_prefixes.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));

        println!("\nBy Prefix:");
        println!("{:-<50}", "");
        println!("{:<30} {:>10} {:>10}", "Prefix", "Count", "Size");
        println!("{:-<50}", "");

        for (prefix, (count, size)) in sorted_prefixes {
            println!("{:<30} {:>10} {:>10}", prefix, count, format_size(*size));
        }
    }

    Ok(())
}

fn extract_prefix(key: &str) -> String {
    if let Some(pos) = key.find("::") {
        let prefix_end = pos + 2;
        if let Some(next_pos) = key[prefix_end..].find("::") {
            key[..prefix_end + next_pos + 2].to_string()
        } else {
            key[..prefix_end].to_string()
        }
    } else {
        "other".to_string()
    }
}

fn format_size(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

fn truncate_key(key: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }

    if key.len() <= max_len {
        key.to_string()
    } else if max_len <= 3 {
        ".".repeat(max_len)
    } else {
        format!("{}...", &key[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_determine_prefix_rejects_conflicting_shortcuts() {
        let err = determine_prefix(None, true, true, false, false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Cannot specify multiple shortcut options"));
    }

    #[test]
    fn regression_determine_prefix_rejects_prefix_plus_shortcut() {
        let err = determine_prefix(Some("artist::"), true, false, false, false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Cannot specify both --prefix and shortcut options"));
    }

    #[test]
    fn integration_determine_prefix_uses_shortcut_constants() {
        let artist_mbid = determine_prefix(None, true, false, false, false).unwrap();
        assert_eq!(
            artist_mbid,
            Some(ARTIST_MBID_CACHE_PREFIX.trim_end_matches("::").to_string())
        );

        let image_meta = determine_prefix(None, false, true, false, false).unwrap();
        assert_eq!(
            image_meta,
            Some(IMAGE_META_CACHE_PREFIX.trim_end_matches("::").to_string())
        );
    }

    #[test]
    fn regression_extract_prefix_handles_unexpected_keys() {
        assert_eq!(extract_prefix("no_delimiter"), "other");
        assert_eq!(extract_prefix("artist::mbid::abc"), "artist::mbid::");
        assert_eq!(extract_prefix("artist::only_one"), "artist::");
    }

    #[test]
    fn regression_truncate_key_handles_small_max_len_without_panic() {
        assert_eq!(truncate_key("abcdef", 0), "");
        assert_eq!(truncate_key("abcdef", 1), ".");
        assert_eq!(truncate_key("abcdef", 2), "..");
        assert_eq!(truncate_key("abcdef", 3), "...");
        assert_eq!(truncate_key("abcdef", 4), "a...");
    }

    #[test]
    fn integration_format_size_boundaries_are_human_readable() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
    }
}
<<<<<<< HEAD

=======
>>>>>>> origin/main
