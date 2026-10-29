//! CLI commands for creative quality auditing, cinematography prompt compiler, and cost optimization.

use std::fs;
use std::path::PathBuf;

use clap::{Arg, ArgMatches, Command};
use retold_app::creative;
use retold_app::creative::{
    CameraOptics, CinematicLighting, ColorGrading, DramaticBeat, ModelTarget, PromptEnhanceOptions,
    Shot,
};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

fn creative_subcommands() -> Vec<Command> {
    vec![
        audit_subcommand(),
        prompt_subcommand(),
        optimize_subcommand(),
        beats_subcommand(),
        benchmark_subcommand(),
        dossier_subcommand(),
        bible_subcommand(),
        coverage_subcommand(),
        storyboard_subcommand(),
        audio_subcommand(),
    ]
}

pub fn command() -> Command {
    Command::new("creative")
        .about("Creative quality auditing, prompt cinematography expansion, and cost optimization")
        .subcommand_required(true)
        .subcommands(creative_subcommands())
}

fn audit_subcommand() -> Command {
    Command::new("audit")
        .about(
            "Audit narrative prose for banned AI cliches, sensory verb density, and oral cadence",
        )
        .args([
            Arg::new("text").help("Narration text or story draft to audit"),
            Arg::new("file")
                .long("file")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to script file to audit"),
        ])
}

fn prompt_target_arg() -> Arg {
    let mut arg = Arg::new("target").long("target");
    arg = arg.value_parser(["flux", "midjourney", "sdxl", "recraft", "generic"]);
    arg = arg.default_value("flux");
    arg.help("Target generation model architecture")
}

fn prompt_lighting_arg() -> Arg {
    let mut arg = Arg::new("lighting").long("lighting");
    arg = arg.value_parser([
        "rembrandt",
        "volumetric",
        "golden-hour",
        "chiaroscuro",
        "neon-noir",
        "soft-diffused",
    ]);
    arg = arg.default_value("rembrandt");
    arg.help("Cinematographic lighting key")
}

fn prompt_optics_arg() -> Arg {
    let mut arg = Arg::new("optics").long("optics");
    arg = arg.value_parser(["anamorphic", "prime", "wide", "telephoto", "macro"]);
    arg = arg.default_value("anamorphic");
    arg.help("Camera lens optical characteristics")
}

fn prompt_grade_arg() -> Arg {
    let mut arg = Arg::new("grade").long("grade");
    arg = arg.value_parser(["portra", "technicolor", "moody", "teal-orange", "bleach"]);
    arg = arg.default_value("portra");
    arg.help("Color grading and filmic stock preset")
}

fn prompt_args() -> Vec<Arg> {
    let description = Arg::new("description")
        .required(true)
        .help("Base visual description of the shot or subject");
    vec![
        description,
        prompt_target_arg(),
        prompt_lighting_arg(),
        prompt_optics_arg(),
        prompt_grade_arg(),
    ]
}

fn prompt_subcommand() -> Command {
    Command::new("prompt")
        .about(
            "Compile raw shot description into production-grade cinematic image generation prompt",
        )
        .args(prompt_args())
}

fn beats_subcommand() -> Command {
    Command::new("beats")
        .about("Evaluate dramatic beat sequence for narrative tension (ΔE) and static scenes")
        .args([
            Arg::new("file")
                .long("file")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to JSON file containing dramatic beats"),
            Arg::new("benchmark")
                .long("benchmark")
                .action(clap::ArgAction::SetTrue)
                .help("Evaluate against standardized Storybit gold-standard benchmark arc"),
        ])
}

fn benchmark_subcommand() -> Command {
    Command::new("benchmark")
        .about("Run calibrated Storybit dramatic arc benchmark for narrative tension calibration")
}

fn dossier_subcommand() -> Command {
    Command::new("dossier")
        .about(
            "Audit research dossier for source citations, chronological beats, and iconic details",
        )
        .args([
            Arg::new("file")
                .long("file")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to research dossier JSON file"),
            Arg::new("benchmark")
                .long("benchmark")
                .action(clap::ArgAction::SetTrue)
                .help("Inspect canonical Dumas 1844 Monte Cristo benchmark research dossier"),
        ])
}

fn bible_subcommand() -> Command {
    Command::new("bible")
        .about(
            "Audit character bible profiles, emotional arcs, and non-likeness visual look prompts",
        )
        .args([
            Arg::new("file")
                .long("file")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to character bible JSON file"),
            Arg::new("benchmark")
                .long("benchmark")
                .action(clap::ArgAction::SetTrue)
                .help("Inspect canonical Monte Cristo character bible benchmark"),
        ])
}

fn coverage_subcommand() -> Command {
    Command::new("coverage")
        .about("Audit script adaptation coverage against planned beats and iconic setups/payoffs")
        .args([Arg::new("benchmark")
            .long("benchmark")
            .action(clap::ArgAction::SetTrue)
            .help("Run benchmark adaptation coverage analysis")])
}

fn storyboard_subcommand() -> Command {
    Command::new("storyboard")
        .about("Audit storyboard camera choreography, optical packages, and 2.5D parallax motion")
        .args([Arg::new("benchmark")
            .long("benchmark")
            .action(clap::ArgAction::SetTrue)
            .help("Run benchmark Monte Cristo storyboard camera choreography audit")])
}

fn audio_subcommand() -> Command {
    Command::new("audio")
        .about("Audit audio soundscape ambience, musical score stings, and narration delivery")
        .args([Arg::new("benchmark")
            .long("benchmark")
            .action(clap::ArgAction::SetTrue)
            .help("Run benchmark Monte Cristo soundscape and score audit")])
}

fn optimize_subcommand() -> Command {
    Command::new("optimize")
        .about("Analyze storyboard shots for frame re-use and 2.5D parallax cost reduction")
        .args([Arg::new("storyboard")
            .long("storyboard")
            .value_parser(clap::value_parser!(PathBuf))
            .help("Path to storyboard JSON file")])
}

fn parse_model_target(raw: &str) -> ModelTarget {
    match raw {
        "midjourney" => ModelTarget::Midjourney,
        "sdxl" => ModelTarget::Sdxl,
        "recraft" => ModelTarget::Recraft,
        _ => ModelTarget::Flux,
    }
}

fn parse_extended_lighting(raw: &str) -> CinematicLighting {
    match raw {
        "chiaroscuro" => CinematicLighting::Chiaroscuro,
        "neon-noir" => CinematicLighting::NeonNoir,
        _ => CinematicLighting::SoftDiffused,
    }
}

fn parse_lighting(raw: &str) -> CinematicLighting {
    match raw {
        "volumetric" => CinematicLighting::VolumetricGodRays,
        "golden-hour" => CinematicLighting::GoldenHour,
        "rembrandt" => CinematicLighting::Rembrandt,
        _ => parse_extended_lighting(raw),
    }
}

fn parse_extended_optics(raw: &str) -> CameraOptics {
    match raw {
        "telephoto" => CameraOptics::Telephoto85mm,
        _ => CameraOptics::Macro100mm,
    }
}

fn parse_optics(raw: &str) -> CameraOptics {
    match raw {
        "prime" => CameraOptics::Prime50mm,
        "wide" => CameraOptics::WideAngle24mm,
        "anamorphic" => CameraOptics::Anamorphic35mm,
        _ => parse_extended_optics(raw),
    }
}

fn parse_extended_grade(raw: &str) -> ColorGrading {
    match raw {
        "teal-orange" => ColorGrading::TealAndOrange,
        _ => ColorGrading::BleachBypass,
    }
}

fn parse_color_grade(raw: &str) -> ColorGrading {
    match raw {
        "technicolor" => ColorGrading::TechnicolorVibrant,
        "moody" => ColorGrading::MoodyDesaturated,
        "portra" => ColorGrading::KodakPortra400,
        _ => parse_extended_grade(raw),
    }
}

fn build_prompt_options(sub_args: &ArgMatches) -> PromptEnhanceOptions {
    let target_str = sub_args
        .get_one::<String>("target")
        .map_or("flux", String::as_str);
    let lighting_str = sub_args
        .get_one::<String>("lighting")
        .map_or("rembrandt", String::as_str);
    let optics_str = sub_args
        .get_one::<String>("optics")
        .map_or("anamorphic", String::as_str);
    let grade_str = sub_args
        .get_one::<String>("grade")
        .map_or("portra", String::as_str);

    PromptEnhanceOptions {
        target: parse_model_target(target_str),
        lighting: parse_lighting(lighting_str),
        optics: parse_optics(optics_str),
        color_grade: parse_color_grade(grade_str),
    }
}

fn resolve_audit_text(sub_args: &ArgMatches) -> Result<String, Diagnostic> {
    if let Some(file_path) = sub_args.get_one::<PathBuf>("file") {
        let error_msg = format!("Failed to read script file: {}", file_path.display());
        return fs::read_to_string(file_path)
            .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, &error_msg));
    }

    if let Some(text_arg) = sub_args.get_one::<String>("text") {
        return Ok(text_arg.clone());
    }

    Err(Diagnostic::new(
        ErrorCode::InputRequired,
        "Provide either narration text or --file <path> to audit.",
    ))
}

fn dispatch_audit(sub_args: &ArgMatches) -> Payload {
    match resolve_audit_text(sub_args) {
        Ok(text) => creative::query_audit(&text),
        Err(diag) => diag.into_payload(),
    }
}

fn dispatch_prompt(sub_args: &ArgMatches) -> Payload {
    let Some(desc) = sub_args.get_one::<String>("description") else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Provide a shot description to enhance.",
        )
        .into_payload();
    };

    let options = build_prompt_options(sub_args);
    creative::query_prompt_enhance(desc, &options)
}

fn parse_storyboard_file(board_path: &PathBuf) -> Result<Vec<Shot>, Diagnostic> {
    let error_msg = format!("Failed to read storyboard file: {}", board_path.display());
    let contents = fs::read_to_string(board_path)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, &error_msg))?;

    serde_json::from_str::<Vec<Shot>>(&contents).map_err(|err| {
        let parse_msg = format!("Failed to parse storyboard shots JSON: {err}");
        Diagnostic::new(ErrorCode::DataInvalid, &parse_msg)
    })
}

fn dispatch_optimize(sub_args: &ArgMatches) -> Payload {
    if let Some(board_path) = sub_args.get_one::<PathBuf>("storyboard") {
        match parse_storyboard_file(board_path) {
            Ok(shots) => creative::query_cost_optimize(&shots),
            Err(diag) => diag.into_payload(),
        }
    } else {
        creative::query_cost_optimize(&[])
    }
}

fn parse_beats_file(beats_path: &PathBuf) -> Result<Vec<DramaticBeat>, Diagnostic> {
    let error_msg = format!("Failed to read beats file: {}", beats_path.display());
    let contents = fs::read_to_string(beats_path)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, &error_msg))?;

    serde_json::from_str::<Vec<DramaticBeat>>(&contents).map_err(|err| {
        let parse_msg = format!("Failed to parse dramatic beats JSON: {err}");
        Diagnostic::new(ErrorCode::DataInvalid, &parse_msg)
    })
}

fn dispatch_beats(sub_args: &ArgMatches) -> Payload {
    if sub_args.get_flag("benchmark") {
        return creative::query_benchmark();
    }

    if let Some(beats_path) = sub_args.get_one::<PathBuf>("file") {
        match parse_beats_file(beats_path) {
            Ok(beats) => creative::query_beats(&beats),
            Err(diag) => diag.into_payload(),
        }
    } else {
        Diagnostic::new(
            ErrorCode::InputRequired,
            "Provide --file <path> to beats JSON or use --benchmark.",
        )
        .into_payload()
    }
}

#[must_use]
pub fn query(_global_workspace: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    match args.subcommand() {
        Some(("audit", sub_args)) => dispatch_audit(sub_args),
        Some(("prompt", sub_args)) => dispatch_prompt(sub_args),
        Some(("optimize", sub_args)) => dispatch_optimize(sub_args),
        Some(("beats", sub_args)) => dispatch_beats(sub_args),
        Some(("benchmark", _)) => creative::query_benchmark(),
        Some(("dossier", sub_args)) => dispatch_dossier(sub_args),
        Some(("bible", sub_args)) => dispatch_bible(sub_args),
        Some(("coverage", sub_args)) => dispatch_coverage(sub_args),
        Some(("storyboard", sub_args)) => dispatch_storyboard(sub_args),
        Some(("audio", sub_args)) => dispatch_audio(sub_args),
        _ => Diagnostic::new(
            ErrorCode::InputRequired,
            "Unknown creative subcommand. Available: audit, prompt, optimize, beats, benchmark, dossier, bible, coverage.",
        )
        .into_payload(),
    }
}

fn dispatch_dossier(_sub_args: &ArgMatches) -> Payload {
    creative::query_benchmark_dossier()
}

fn dispatch_bible(_sub_args: &ArgMatches) -> Payload {
    creative::query_benchmark_bible()
}

fn dispatch_coverage(_sub_args: &ArgMatches) -> Payload {
    creative::query_benchmark_coverage()
}

fn dispatch_storyboard(_sub_args: &ArgMatches) -> Payload {
    creative::query_benchmark_storyboard()
}

fn dispatch_audio(_sub_args: &ArgMatches) -> Payload {
    creative::query_benchmark_soundscape()
}
