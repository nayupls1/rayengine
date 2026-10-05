#!/usr/bin/env bash
# User-level checkout install; compatible with macOS's Bash 3.2.
set -euo pipefail

fail() { printf 'rayengine setup: %s\n' "$*" >&2; exit 1; }
shell_quote() {
    # Keep replacement text in variables: Bash 3.2 parses quote literals in
    # substitution expressions differently from current Bash.
    local single_quote="'" escaped_quote="'\\''" value=$1
    printf "'%s'" "${value//"$single_quote"/$escaped_quote}"
}
usage() {
    cat <<'HELP'
Usage: bash scripts/setup.sh [--root DIR] [--shell bash|zsh|fish|sh] [--debug]
       bash scripts/setup.sh [--shell bash|zsh|fish|sh] --remove-path

Install this checkout's CLI and persist its bin directory for your login shell.
Root: --root, then CARGO_INSTALL_ROOT, then CARGO_HOME, then ~/.cargo.
Cargo's install.root config is deliberately overridden by this explicit root.
--debug builds faster for development; the default is a release build.
--remove-path reverses only setup's shell changes; it does not uninstall binaries.
HELP
}

install_root=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}
shell_name=${SHELL:-sh}
shell_name=${shell_name##*/}
shell_name=${shell_name:-sh}
remove_path=false
debug=false
while (( $# )); do
    case $1 in
        --root|--shell)
            (( $# >= 2 )) || fail "$1 needs a value (see --help)."
            case $1 in --root) install_root=$2;; --shell) shell_name=$2;; esac
            shift 2 ;;
        --remove-path) remove_path=true; shift ;;
        --debug) debug=true; shift ;;
        --help|-h) usage; exit 0 ;;
        *) fail "Unknown option: $1 (see --help)." ;;
    esac
done

case $shell_name in
    bash)
        files=("$HOME/.bashrc")
        if $remove_path; then
            # Login-file precedence may have changed since setup ran.
            files+=("$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile")
        # Bash uses only the first existing login file in this order.
        elif [[ -e $HOME/.bash_profile ]]; then files+=("$HOME/.bash_profile")
        elif [[ -e $HOME/.bash_login ]]; then files+=("$HOME/.bash_login")
        else files+=("$HOME/.profile"); fi ;;
    zsh) files=("${ZDOTDIR:-$HOME}/.zshrc" "${ZDOTDIR:-$HOME}/.zprofile") ;;
    fish) files=("${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/rayengine-cli.fish") ;;
    sh) files=("$HOME/.profile") ;;
    *) fail "Unsupported shell '$shell_name'. Use --shell bash, zsh, fish or sh; see docs/cli_setup.md." ;;
esac

# Leading newline belongs to the block: removal restores even a file without
# a final newline. Use a sentinel when reading to preserve all trailing newlines.
begin=$'\n# >>> rayengine CLI PATH >>>\n'
end=$'# <<< rayengine CLI PATH <<<\n'
read_configs() {
    originals=()
    cleaned=()
    for file in "${files[@]}"; do
        content=''
        if [[ -e $file ]]; then
            [[ -f $file && -r $file && -w $file ]] || fail "Cannot edit $file; check its permissions."
            content=$(cat -- "$file"; printf '\001')
            content=${content%$'\001'}
        elif [[ -L $file ]]; then
            fail "Startup file is a broken symlink: $file. Repair it first."
        fi
        originals+=("$content")
        if [[ $content == *'# >>> rayengine CLI PATH >>>'* ]]; then
            [[ $content == *"$begin"* && $content == *"$end"* ]] || fail "Incomplete setup block in $file; repair its markers before retrying."
            before=${content%%"$begin"*}
            rest=${content#*"$begin"}
            [[ $rest == *"$end"* ]] || fail "Incomplete setup block in $file."
            after=${rest#*"$end"}
            content=$before$after
        fi
        [[ $content != *'# >>> rayengine CLI PATH >>>'* && $content != *'# <<< rayengine CLI PATH <<<'* ]] || fail "Unexpected setup markers in $file; repair them before retrying."
        cleaned+=("$content")
    done
}
read_configs

if ! $remove_path; then
    [[ -n $install_root ]] || fail 'Install root cannot be empty.'
    case $install_root in /*) ;; *) install_root=$PWD/$install_root;; esac
    # A colon cannot be represented as a POSIX PATH entry; neither can newlines
    # safely be embedded in our startup blocks.
    [[ $install_root != *:* && $install_root != *$'\n'* && $install_root != *$'\r'* ]] || fail 'Install root must not contain colons or newlines.'
    for tool in rustc cargo; do
        command -v "$tool" >/dev/null 2>&1 || fail "Missing $tool. Install Rust 1.89+ from https://rustup.rs, reopen the terminal, and retry."
    done
    rust_version=$(rustc --version) || fail 'rustc failed. Repair/select your Rust toolchain with rustup and retry.'
    rust_regex='^rustc ([0-9]+)\.([0-9]+)\.'
    [[ $rust_version =~ $rust_regex ]] || fail "Cannot read Rust version: $rust_version"
    (( BASH_REMATCH[1] > 1 || (BASH_REMATCH[1] == 1 && BASH_REMATCH[2] >= 89) )) || fail "Rust 1.89+ required; found $rust_version. Run rustup update stable."
    cargo --version >/dev/null || fail 'Cargo failed. Repair/select your Rust toolchain with rustup and retry.'
    mkdir -p -- "$install_root" || fail "Cannot create install root $install_root; choose a writable --root."
    install_root=$(cd -- "$install_root" && pwd -P)
    [[ $install_root != *:* && $install_root != *$'\n'* && $install_root != *$'\r'* ]] || fail 'Resolved install root must not contain colons or newlines.'
    bin_dir=$install_root/bin
    repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
    args=(install --locked --path "$repo_root/crates/rayengine-cli" --root "$install_root" --bin rayengine)
    if $debug; then args+=(--debug); fi
    printf 'Installing checkout CLI into %s\n' "$bin_dir"
    # Force the host toolchain rather than an inherited cross-compilation target.
    host=$(rustc -vV | sed -n 's/^host: //p')
    [[ -n $host ]] || fail 'Cannot detect the Rust host target; check rustc -vV.'
    if ! cargo "${args[@]}" --target "$host"; then
        fail 'Cargo installation failed; see diagnostics above. Check your linker, network, checkout lockfile and root permissions, then rerun setup. No PATH changes were made.'
    fi
    [[ -x $bin_dir/rayengine ]] || fail "Cargo succeeded but $bin_dir/rayengine is missing or not executable. No PATH changes were made."
    "$bin_dir/rayengine" --help >/dev/null || fail 'Installed CLI cannot run. No PATH changes were made.'
    # Single-quote literal paths, including quotes, $, backticks and spaces.
    quoted=$(shell_quote "$bin_dir")
    if [[ $shell_name == fish ]]; then
        # Fish single quotes escape backslashes and single quotes differently.
        fish_path=${bin_dir//\\/\\\\}
        fish_path=${fish_path//\'/\\\'}
        block="if not contains -- '$fish_path' \$PATH"$'\n'"    set -gx PATH '$fish_path' \$PATH"$'\nend\n'
    else
        block="case \":\${PATH-}:\" in"$'\n'"    *:$quoted:*) ;;"$'\n'"    *) export PATH=$quoted\${PATH:+:\$PATH} ;;"$'\nesac\n'
    fi
fi

# Cargo can take minutes; preserve edits made while it was building.
if ! $remove_path; then read_configs; fi
for (( i=0; i<${#files[@]}; i++ )); do
    file=${files[i]}
    replacement=${cleaned[i]}
    if ! $remove_path; then replacement=$replacement$begin$block$end; fi
    if [[ $replacement != "${originals[i]}" ]]; then
        mkdir -p -- "$(dirname -- "$file")" || fail "Cannot create shell config directory for $file."
        # Writing through the file preserves existing permissions and symlinks.
        printf '%s' "$replacement" > "$file" || fail "Cannot write $file; check permissions."
        printf 'Updated %s\n' "$file"
    fi
done
if $remove_path; then
    printf 'Removed setup PATH blocks. Open a new terminal to use the remaining shell configuration.\n'
else
    printf 'Installed: %s\n' "$("$bin_dir/rayengine" --version)"
    printf 'Activate in this terminal:\n'
    if [[ $shell_name == fish ]]; then
        printf "  contains -- '%s' \$PATH; or set -gx PATH '%s' \$PATH\n" "$fish_path" "$fish_path"
    else
        # Print the expansion literally for the parent shell to evaluate.
        # shellcheck disable=SC2016
        printf '  export PATH=%s${PATH:+:$PATH}\n' "$quoted"
    fi
    printf 'Then open a NEW terminal and run: rayengine --help\n'
    printf 'If another installation wins command lookup, inspect it with: %s\n' 'command -v rayengine'
    root_quoted=$(shell_quote "$install_root")
    printf 'Uninstall the binary: cargo uninstall rayengine-cli --root %s\n' "$root_quoted"
fi
