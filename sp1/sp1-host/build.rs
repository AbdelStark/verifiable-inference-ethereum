use sp1_build::{BuildArgs, build_program_with_args};

fn main() {
    build_program_with_args(
        "../segment-guest",
        BuildArgs {
            locked: true,
            ..Default::default()
        },
    );
}
