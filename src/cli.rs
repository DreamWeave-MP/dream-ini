use std::path::PathBuf;

use clap::{ArgAction, Parser};
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[allow(clippy::struct_excessive_bools)]
#[command(
    name = "dream-ini",
    about = "Import Morrowind.ini settings into openmw.cfg",
    version,
    disable_help_flag = true,
    disable_version_flag = true,
    override_usage = "dream-ini --ini <FILE> [--cfg <FILE>] [--output <FILE>|--in-place] [options]\n       dream-ini --generate-completion <SHELL>\n       dream-ini --generate-manpage",
    after_help = "Import mode requires --ini <FILE>. Optional --cfg <FILE> is read as the base config; without it, import starts empty. Default output is cfg text on stdout with diagnostics on stderr. Use --output <FILE> to write a cfg file, or --in-place with --cfg <FILE> to update the base cfg. Relative --data is resolved from the output cfg directory, from --cfg for stdout preview, or from the current directory and written absolute when stdout has no cfg context. Non-import modes (--help, --version, --generate-completion, and --generate-manpage) do not require --ini."
)]
pub(crate) struct Cli {
    /// Verbose output
    #[arg(short, long, display_order = 13)]
    pub(crate) verbose: bool,

    /// Print help
    #[arg(short, long, action = ArgAction::Help, display_order = 6)]
    pub(crate) help: Option<bool>,

    /// Print version
    #[arg(short = 'V', long, action = ArgAction::Version, display_order = 17)]
    pub(crate) version: Option<bool>,

    /// Morrowind.ini file
    #[arg(
        short,
        long,
        value_name = "FILE",
        display_order = 7,
        required_unless_present_any = ["generate_completion", "generate_manpage"]
    )]
    pub(crate) ini: Option<PathBuf>,

    /// openmw.cfg file
    #[arg(short, long, value_name = "FILE", display_order = 1)]
    pub(crate) cfg: Option<PathBuf>,

    /// Output openmw.cfg file
    #[arg(
        short = 'o',
        long,
        value_name = "FILE",
        display_order = 10,
        conflicts_with_all = ["in_place"]
    )]
    pub(crate) output: Option<PathBuf>,

    /// Explicit Data Files directory to search
    #[arg(short = 'd', long = "data", value_name = "DIR", display_order = 2)]
    pub(crate) data_dir: Option<PathBuf>,

    /// Set data-local in the imported cfg, replacing any existing value
    #[arg(
        short = 'l',
        long = "data-local",
        value_name = "DIR",
        display_order = 8
    )]
    pub(crate) data_local: Option<PathBuf>,

    /// Set resources in the imported cfg, replacing any existing value
    #[arg(short, long, value_name = "DIR", display_order = 11)]
    pub(crate) resources: Option<PathBuf>,

    /// Set user-data in the imported cfg, replacing any existing value
    #[arg(short, long = "user-data", value_name = "DIR", display_order = 12)]
    pub(crate) user_data: Option<PathBuf>,

    /// Write the imported result back to the --cfg file
    #[arg(
        short = 'w',
        long,
        display_order = 14,
        requires = "cfg",
        conflicts_with_all = ["output"]
    )]
    pub(crate) in_place: bool,

    /// Generate shell completion script to stdout
    #[arg(
        short = 'C',
        long,
        value_name = "SHELL",
        display_order = 15,
        conflicts_with_all = [
            "generate_manpage",
            "ini",
            "cfg",
            "output",
            "data_dir",
            "data_local",
            "resources",
            "user_data",
            "in_place",
            "game_files",
            "fonts",
            "no_archives",
            "encoding",
            "verbose"
        ]
    )]
    pub(crate) generate_completion: Option<Shell>,

    /// Generate roff manpage to stdout
    #[arg(
        short = 'M',
        long,
        display_order = 16,
        conflicts_with_all = [
            "generate_completion",
            "ini",
            "cfg",
            "output",
            "data_dir",
            "data_local",
            "resources",
            "user_data",
            "in_place",
            "game_files",
            "fonts",
            "no_archives",
            "encoding",
            "verbose"
        ]
    )]
    pub(crate) generate_manpage: bool,

    /// Import esm and esp files
    #[arg(short = 'g', long = "game-files", display_order = 5)]
    pub(crate) game_files: bool,

    /// Import bitmap fonts
    #[arg(short, long, display_order = 4)]
    pub(crate) fonts: bool,

    /// Disable bsa archives import
    #[arg(short = 'n', long = "no-archives", display_order = 8)]
    pub(crate) no_archives: bool,

    /// Character encoding for imported content-file names: win1250, win1251, or win1252
    #[arg(short, long, value_name = "ENCODING", display_order = 3)]
    pub(crate) encoding: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn accepts_flag_ini_and_cfg() {
        let cli = Cli::parse_from(["dream-ini", "--ini", "mw.ini", "--cfg", "openmw.cfg"]);

        assert_eq!(cli.ini, Some(PathBuf::from("mw.ini")));
        assert_eq!(cli.cfg, Some(PathBuf::from("openmw.cfg")));
    }

    #[test]
    fn rejects_positional_paths() {
        let error = Cli::try_parse_from(["dream-ini", "Morrowind.ini", "openmw.cfg"]).unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn clap_requires_ini_for_import_options() {
        let error = Cli::try_parse_from(["dream-ini", "--output", "openmw.cfg"]).unwrap_err();

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn parses_import_options() {
        let cli = Cli::parse_from([
            "dream-ini",
            "--game-files",
            "--fonts",
            "-n",
            "--encoding",
            "win1251",
            "-d",
            "Data Files",
            "-l",
            "local-data",
            "-r",
            "resources",
            "-u",
            "user-data",
            "--output",
            "out.cfg",
            "--ini",
            "mw.ini",
            "--cfg",
            "openmw.cfg",
        ]);

        assert!(cli.game_files);
        assert!(cli.fonts);
        assert!(cli.no_archives);
        assert!(!cli.in_place);
        assert_eq!(cli.data_dir, Some(PathBuf::from("Data Files")));
        assert_eq!(cli.data_local, Some(PathBuf::from("local-data")));
        assert_eq!(cli.resources, Some(PathBuf::from("resources")));
        assert_eq!(cli.user_data, Some(PathBuf::from("user-data")));
        assert_eq!(cli.encoding.as_deref(), Some("win1251"));
        assert_eq!(cli.output, Some(PathBuf::from("out.cfg")));
    }

    #[test]
    fn parses_in_place_output_mode() {
        let cli = Cli::parse_from([
            "dream-ini",
            "--ini",
            "Morrowind.ini",
            "--cfg",
            "openmw.cfg",
            "--in-place",
        ]);

        assert!(cli.in_place);
        assert_eq!(cli.cfg, Some(PathBuf::from("openmw.cfg")));
        assert_eq!(cli.output, None);
    }

    #[test]
    fn parses_short_output_modes() {
        let output = Cli::parse_from(["dream-ini", "--ini", "Morrowind.ini", "-o", "out.cfg"]);
        let in_place = Cli::parse_from([
            "dream-ini",
            "--ini",
            "Morrowind.ini",
            "--cfg",
            "openmw.cfg",
            "-w",
        ]);

        assert_eq!(output.output, Some(PathBuf::from("out.cfg")));
        assert!(in_place.in_place);
    }

    #[test]
    fn in_place_requires_cfg_at_parse_time() {
        let error =
            Cli::try_parse_from(["dream-ini", "--ini", "Morrowind.ini", "--in-place"]).unwrap_err();

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn in_place_conflicts_with_other_output_modes_at_parse_time() {
        let output_error = Cli::try_parse_from([
            "dream-ini",
            "--ini",
            "Morrowind.ini",
            "--cfg",
            "openmw.cfg",
            "--in-place",
            "--output",
            "out.cfg",
        ])
        .unwrap_err();
        assert_eq!(
            output_error.kind(),
            clap::error::ErrorKind::ArgumentConflict
        );
    }

    #[test]
    fn rejects_repeated_data_path() {
        let error = Cli::try_parse_from([
            "dream-ini",
            "--ini",
            "Morrowind.ini",
            "--data",
            "Data Files",
            "--data",
            "Other Data",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn help_mentions_core_options() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("--game-files"));
        assert!(help.contains("--fonts"));
        assert!(help.contains("--no-archives"));
        assert!(help.contains("--encoding"));
        assert!(help.contains("--output"));
        assert!(help.contains("--data"));
        assert!(help.contains("--data-local"));
        assert!(help.contains("--resources"));
        assert!(help.contains("--user-data"));
        assert!(help.contains("--in-place"));
        assert!(help.contains("--generate-completion"));
        assert!(help.contains("--generate-manpage"));

        let ordered_options = [
            "-c, --cfg",
            "-d, --data",
            "-e, --encoding",
            "-f, --fonts",
            "-g, --game-files",
            "-h, --help",
            "-i, --ini",
            "-l, --data-local",
            "-n, --no-archives",
            "-o, --output",
            "-r, --resources",
            "-u, --user-data",
            "-v, --verbose",
            "-w, --in-place",
            "-C, --generate-completion",
            "-M, --generate-manpage",
            "-V, --version",
        ];
        let mut previous_position = 0;
        for option in ordered_options {
            let position = help
                .find(option)
                .unwrap_or_else(|| panic!("missing {option}"));
            assert!(position >= previous_position, "{option} is out of order");
            previous_position = position;
        }
    }

    #[test]
    fn rejects_old_no_archives_short_flag() {
        let error = Cli::try_parse_from(["dream-ini", "--ini", "Morrowind.ini", "-A"]).unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn parses_generation_options() {
        let completion = Cli::parse_from(["dream-ini", "--generate-completion", "bash"]);
        assert_eq!(completion.generate_completion, Some(Shell::Bash));

        let manpage = Cli::parse_from(["dream-ini", "--generate-manpage"]);
        assert!(manpage.generate_manpage);

        let short_completion = Cli::parse_from(["dream-ini", "-C", "bash"]);
        assert_eq!(short_completion.generate_completion, Some(Shell::Bash));

        let short_manpage = Cli::parse_from(["dream-ini", "-M"]);
        assert!(short_manpage.generate_manpage);
    }
}
