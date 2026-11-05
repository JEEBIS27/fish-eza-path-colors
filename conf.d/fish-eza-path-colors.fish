# Optional protocol: stock fish keeps its normal syntax highlighting.
if test "$fish_path_style_provider_protocol" != 1
    return
end
set -l provider (command -s fish-eza-path-colors-provider)
test -n "$provider"; or return
set -g fish_color_path_style_provider "$provider"
set -g fish_color_path_style_provider_environment EZA_COLORS LS_COLORS EXA_COLORS EZA_CONFIG_DIR XDG_CONFIG_HOME HOME

function __fish_eza_path_colors_apply --on-variable fish_eza_path_colors_enabled
    set -g fish_color_path_style_provider_enabled "$fish_eza_path_colors_enabled"
end
# Backward-compatible switch used by the original local implementation.
function __fish_eza_path_colors_legacy --on-variable fish_color_path_by_type
    if set -q fish_color_path_by_type
        set -g fish_eza_path_colors_enabled "$fish_color_path_by_type"
    end
end
if not set -q fish_eza_path_colors_enabled
    if set -q fish_color_path_by_type
        set -g fish_eza_path_colors_enabled "$fish_color_path_by_type"
    else
        set -g fish_eza_path_colors_enabled 1
    end
end
__fish_eza_path_colors_apply
