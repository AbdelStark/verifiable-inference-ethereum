use sp1_build::{BuildArgs, build_program_with_args};

fn main() {
    for program in [
        "../segment-guest",
        "../aggregator-guest",
        "../echo-guest",
        "../stwo-bench",
    ] {
        build_program_with_args(
            program,
            BuildArgs {
                locked: true,
                ..Default::default()
            },
        );
    }
}
