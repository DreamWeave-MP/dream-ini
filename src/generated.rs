use std::io::Write;

use clap::CommandFactory;

use crate::cli::Cli;
use crate::command::CliError;

pub(crate) fn handle_generated_output(cli: &Cli, stdout: &mut dyn Write) -> Result<bool, CliError> {
    if let Some(shell) = cli.generate_completion {
        clap_complete::generate(shell, &mut Cli::command(), "dream-ini", stdout);
        return Ok(true);
    }

    if cli.generate_manpage {
        render_manpage(stdout)?;
        return Ok(true);
    }

    Ok(false)
}

fn render_manpage(stdout: &mut dyn Write) -> Result<(), CliError> {
    let manpage = clap_mangen::Man::new(Cli::command());
    manpage.render_title(stdout)?;
    manpage.render_name_section(stdout)?;
    write!(
        stdout,
        ".SH SYNOPSIS\n.B dream-ini\n--ini <FILE> [--cfg <FILE>] [--output <FILE>|--in-place] [options]\n.br\n.B dream-ini\n--generate-completion <SHELL>\n.br\n.B dream-ini\n--generate-manpage\n.br\n.B dream-ini\ninstall-launcher\n"
    )?;
    manpage.render_description_section(stdout)?;
    manpage.render_options_section(stdout)?;
    manpage.render_extra_section(stdout)?;
    manpage.render_version_section(stdout)?;
    Ok(())
}
