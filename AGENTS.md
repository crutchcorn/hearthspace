# Permissions

You are fully sandboxed in an Ubuntu 26.04 LTS Desktop VM. The VM is being manually snapshot at regular intervals. No other work is being done in this VM other than creating a new desktop environment.

You may use `git commit` and any other Git operations.

Sudo is set up without a password and is allowed to be used.

Software may be installed, modified, or anything in-between. However, if changes to the OS are required outside of code present in this folder, we need to document it in `SETUP.md` at the root of this project.

Rust libraries are not only allowed, but encouraged to be used if they will solve problems for our needs. 

# Relevant Research

## GPUI

GPUI (Rust UI system) powers our shell's UI but not the compositor. For more information, see [our docs on UI](./docs/UI.md)

GPUI is not well documented for Agents on the web, so I pulled in the codebase locally. You can find the project at `~/git/zed-industries/zed/crates/gpui`. There's no need to pull in that context eagerly, but feel free to freely search the code if you have any questions.