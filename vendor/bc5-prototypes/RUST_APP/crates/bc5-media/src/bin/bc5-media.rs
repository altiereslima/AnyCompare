use bc5_media::{audit_golden_evidence, EvidenceCase, ObservedFieldCase, audit_by_field,observe_media_interpreted, observe_media_raw, compare_raw_observations, compare_candidate_policies, compare_media, extract_cover_art, inspect_media, render_html_with_options, render_text_with_options, to_json, FieldStatus, InspectOptions, MediaProfile, MediaRules, ReportOptions, ReportLayout, ReportDisplay};
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name="bc5-media", version, about="Media Compare independente em Rust (MP3, FLAC, MP4/AAC)")]
struct Cli { #[command(subcommand)] cmd: Command }

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format { Text, Html, Json }
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Layout { SideBySide, Summary }
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Display { All, Mismatches, Matches }

#[derive(Debug, Subcommand)]
enum Command {
    /// Diagnose hypotheses using labeled BC5 fields; NEVER declares parity.
    Calibrate { input: PathBuf, #[arg(short,long)] output: Option<PathBuf> },
    /// Confere hashes reais da mídia e dos relatórios usados como observação A/B.
    /// NÃO autentica que o BC5 produziu esses relatórios.
    AuditGolden { root: PathBuf, manifest: PathBuf, #[arg(short,long)] output: Option<PathBuf> },
    /// Dados brutos e offsets de ID3/FLAC/MP4 (forense, NÃO equivale ao BC5).
    RawTags { file: PathBuf, #[arg(short,long)] output: Option<PathBuf> },
    /// Interpretação forense (ID3, Vorbis, MP4), sem decidir equivalência BC5.
    ExplainTags { file: PathBuf, #[arg(short,long)] output: Option<PathBuf> },
    /// Comparacao bruta por identificador e bytes, sem semantica BC5.
    RawCompare { left: PathBuf, right: PathBuf, #[arg(short,long)] output: Option<PathBuf> },
    /// Hypotheses for duplicates/order: shows disagreements; NEVER BC5 parity.
    CandidateCompare { left: PathBuf, right: PathBuf, #[arg(short,long)] output: Option<PathBuf> },
    /// Print normalized tags and technical media properties.
    Inspect { file: PathBuf, #[arg(long)] hash: bool, #[arg(long)] json: bool, #[arg(long)] extended: bool },
    /// Compare media metadata side by side.
    Compare {
        left: PathBuf, right: PathBuf,
        #[arg(long)] rules: Option<PathBuf>,
        #[arg(long, value_enum, default_value="text")] format: Format,
        #[arg(short, long)] output: Option<PathBuf>,
        #[arg(long)] ignore_unimportant: bool,
        #[arg(long)] differences_only: bool,
        #[arg(long)] ignore_case: bool,
        #[arg(long)] trim_whitespace: bool,
        #[arg(long)] hash: bool,
        /// Allow additional formats supported by Lofty, not officially supported by BC5.
        #[arg(long)] extended: bool,
        #[arg(long, value_enum, default_value="side-by-side")] layout: Layout,
        #[arg(long, value_enum, default_value="all")] display: Display,
    },
    /// Save an embedded cover without changing the source file.
    Cover { file: PathBuf, #[arg(long,default_value_t=0)] index:usize, #[arg(short,long)] output:PathBuf },
    /// Open a media file in the associated application (NOT BC5's native Play).
    OpenWith { file: PathBuf },
}

fn main() {
    match run() {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(error) => { eprintln!("Erro: {error}"); std::process::exit(2); }
    }
}

fn run() -> Result<i32,Box<dyn std::error::Error>> {
    let cli=Cli::parse();
    match cli.cmd {
        Command::Calibrate { input, output } => {
            let cases: Vec<ObservedFieldCase>=serde_json::from_str(&fs::read_to_string(input)?)?;
            let data=serde_json::to_string_pretty(&audit_by_field(&cases))?;
            if let Some(path)=output {fs::write(path,data.as_bytes())?;}
            else {println!("{data}");}
        },
        Command::AuditGolden { root, manifest, output } => {
            let entries: Vec<EvidenceCase> = serde_json::from_str(&fs::read_to_string(manifest)?)?;
            let audit = audit_golden_evidence(&root, &entries)?;
            let invalid = audit.rejected > 0;
            let data = serde_json::to_string_pretty(&audit)?;
            if let Some(path) = output { fs::write(path, data.as_bytes())?; }
            else { println!("{data}"); }
            if invalid { return Ok(2); }
        },
        Command::ExplainTags { file, output } => {
            let obs=observe_media_interpreted(file)?;
            let data=serde_json::to_string_pretty(&obs)?;
            if let Some(path)=output {fs::write(path,data.as_bytes())?;} else {println!("{data}");}
        },
        Command::RawCompare { left, right, output } => {
            let a=observe_media_raw(left)?;
            let b=observe_media_raw(right)?;
            let report=compare_raw_observations(&a,&b);
            let data=serde_json::to_string_pretty(&report)?;
            if let Some(path)=output { fs::write(path,data.as_bytes())?; } else { println!("{data}"); }
        }
        Command::CandidateCompare { left, right, output } => {
            let a=observe_media_raw(left)?;
            let b=observe_media_raw(right)?;
            let data=serde_json::to_string_pretty(&compare_candidate_policies(&a, &b))?;
            if let Some(path)=output { fs::write(path,data.as_bytes())?; }
            else { println!("{data}"); }
        },
        Command::RawTags { file, output } => {
            let obs=observe_media_raw(file)?;
            let data=serde_json::to_string_pretty(&obs)?;
            if let Some(path)=output { fs::write(path,data.as_bytes())?; } else { println!("{data}"); }
        }
        Command::Inspect { file, hash, json, extended } => {
            let m=inspect_media(file, InspectOptions { include_file_hash:hash, profile: if extended { MediaProfile::Extended } else { MediaProfile::Bc5 } })?;
            if json { println!("{}",serde_json::to_string_pretty(&m)?); }
            else {
                println!("Arquivo: {}\nFormato: {}\nTamanho: {} B",m.path,m.file_type,m.file_size);
                for (name,values) in &m.fields { println!("{}: {}",name,values.join("; ")); }
                for w in &m.warnings { eprintln!("Aviso: {w}"); }
            }
        }
        Command::Compare { left,right,rules,format,output,ignore_unimportant,differences_only,ignore_case,trim_whitespace,hash,extended,layout,display } => {
            eprintln!("AVISO: comparador semantico EXPERIMENTAL, nao validado como 1:1 com BC5 5.2.7.32913.");
            let mut r=if let Some(p)=rules { serde_json::from_str::<MediaRules>(&fs::read_to_string(p)?)? } else { MediaRules::default() };
            if ignore_unimportant {r.ignore_unimportant=true;}
            if ignore_case {r.ignore_case=true;}
            if trim_whitespace {r.trim_whitespace=true;}
            let opts=InspectOptions { include_file_hash:hash, profile: if extended { MediaProfile::Extended } else { MediaProfile::Bc5 } };
            let (l,rr)=(inspect_media(left,opts)?,inspect_media(right,opts)?);
            let mut comparison=compare_media(&l,&rr,&r);
            let display = if differences_only { Display::Mismatches } else { display };
            let report_options = ReportOptions {
                layout: match layout { Layout::SideBySide => ReportLayout::SideBySide, Layout::Summary => ReportLayout::Summary },
                display: match display { Display::All => ReportDisplay::All, Display::Mismatches => ReportDisplay::Mismatches, Display::Matches => ReportDisplay::Matches },
                ignore_unimportant: r.ignore_unimportant,
            };
            if matches!(format, Format::Json) {
                let show = |f: &bc5_media::FieldComparison| {
                    let different = f.status == FieldStatus::ImportantDifference ||
                        (f.status == FieldStatus::UnimportantDifference && !r.ignore_unimportant);
                    match report_options.display {
                        ReportDisplay::All => true,
                        ReportDisplay::Mismatches => different,
                        ReportDisplay::Matches => !different,
                    }
                };
                if report_options.layout == ReportLayout::Summary { comparison.fields.clear(); }
                else { comparison.fields.retain(show); }
            }
            let data=match format {
                Format::Text => render_text_with_options(&comparison, report_options),
                Format::Html => render_html_with_options(&comparison, report_options),
                Format::Json => to_json(&comparison)?,
            };
            if let Some(path)=output {fs::write(path,data.as_bytes())?;} else {println!("{data}");}
            // Proper exit codes: 0=same, 1=different, 2=I/O/parser/error.
            if comparison.effective_different() { return Ok(1); }
        }
        Command::Cover{file,index,output}=>{
            let p=extract_cover_art(file,index,output)?;
            println!("Capa #{}: {} bytes / {}",p.index,p.size_bytes,p.blake3);
        }
        Command::OpenWith{file}=>open::that(file)?,
    }
    Ok(0)
}
