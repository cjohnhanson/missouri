use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    sigpipe::reset();
    let args = missouri::cli::Args::parse();

    ctrlc::set_handler(|| {
        if missouri::signal::is_interrupted() {
            // Second Ctrl+C: force exit
            missouri::signal::set_force_exit();
            missouri::signal::kill_all_children(libc::SIGKILL);
            std::process::exit(130);
        }
        // First Ctrl+C: graceful shutdown
        missouri::signal::set_interrupted();
        missouri::signal::kill_all_children(libc::SIGTERM);
    })
    .ok();

    // Set up miette for nice error render
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .color(true)
                .build(),
        )
    }))
    .ok(); // ignore if already set

    let result = missouri::cli::run(args);

    // Exit 130 if interrupted (standard SIGINT convention)
    if missouri::signal::is_interrupted() {
        return ExitCode::from(130);
    }

    match result {
        Ok(success) => {
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("{:?}", e);
            ExitCode::from(2)
        }
    }
}
