# binboh

_Building INcrementally Based On Hashes_

Binboh is the smallest unit of an incremental build system that is built on hashes.
It performs a single process call only if the hashes of the input and output files do not match a previous call.
It is intended to be used in larger builds to avoid redundant steps.

## Usage

```shell
binboh \
    -i input_file_1 input_file_2 ... \
    -o output_file_1 output_file_2 ... \
    -- command to run ...
```

Example: `binboh -i data.json analysis.py -o result.json -- python3 analysis.py`

This will run `python3 analysis.py` and cache the hashes of `data.json`, `analysis.py`, and `result.json`.
When called again, `python3 analysis.py` will only be called if either of the three files have changed.

## Installation

`cargo install --git https://github.com/Tillerino/binboh.git`

`~/.cargo/bin` should be in your PATH.

## Details

One _call_ is defined by the current working directory, the input file paths, the output file paths, and the command.
For each call, all input files and output files are hashed to determine if the command needs to run or not.

After each run, the call's hashes are stored in `~/.cache/binboh/` (or equivalent) for future reference - one file per call.
The file name is based on a hash of all properties of the call (not the file contents!).
Changing anything about the call will for the command to be executed again regardless of changes to input or output files.

Try running binboh with the `--verbose` flag. This makes everything quite

## Alternatives

- [Tup](https://github.com/gittup/tup) allows the user to define build graphs and checks the completeness of input and output declarations via a FUSE proxy. However, it is entirely based on timestamps - not hashes.
- [fabricate](https://github.com/brushtechnology/fabricate) is a hash-based build tool. Both the code and the build scripts are Python-based, so it is intended to be the leading build tool, not a smaller part.

