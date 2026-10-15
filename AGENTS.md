# Permissions

You are fully sandboxed in an Ubuntu 26.04 LTS Desktop VM. The VM is being manually snapshot at regular intervals. No other work is being done in this VM other than creating a new desktop environment.

You may use `git commit` and any other Git operations, but never ever use `git push --force`, as it would impact more than the VM.

Sudo is set up without a password and is allowed to be used.

Software may be installed, modified, or anything in-between. However, if changes to the OS are required outside of code present in this folder, we need to document it in `CONTRIBUTING.md` at the root of this project.

Rust libraries are not only allowed, but encouraged to be used if they will solve problems for our needs.

# Code Quality

We should generally aim to keep our files smaller, composible, and battle-tested. To do this:

- Keep files under ~400LOC (move code to a new file/crate/package if it grows too big)
- We should be writing tests incrementally as we develop new features and fixes
- Make sure test/format/lint/build all run fully prior to marking work as "complete"

# Agentic Native Testing

For automated control or validation on a real Linux VT, read the repository skill
[hearthspace-native-control](./.agents/skills/hearthspace-native-control/SKILL.md).
It covers session ownership, screenshots and input, private accessibility access,
and recovery through the native harness. Use it when testing hardware presentation
or desktop interactions; ordinary deterministic regressions can remain headless.

# Relevant Research

## Smithay

Smithay is what our compositor is based off of. We should be asking ourselves "is this something Smithay already handles for us" every time we try to build new functionality that might be useful for any other non-infinite-canvas-based compositor; it likely has it.

Smithay is not well documented for Agents on the web, so I pulled in the codebase locally. You can find the project at `~/git/Smithay/smithay`. There's no need to pull in that context eagerly, but feel free to freely search the code if you have any questions.

## Wayland Protocols

Wayland (and various of the compositors on the market right now) will provide extension protocols to add functionality across many compositors. Before adding any new major features, we should check against the list of them to see if there are any that we should consider adopting:

https://wayland.app/protocols/

## Xilem and Masonry

Xilem/Masonry powers our shell's UI and compositor. Masonry is a part of Xilem. For more information, see [our docs on UI](./docs/UI.md)

Xilem & Masonry are not well documented for Agents on the web, so I pulled in the codebase locally. You can find the project at `~/git/linebender/xilem`. There's no need to pull in that context eagerly, but feel free to freely search the code if you have any questions.
