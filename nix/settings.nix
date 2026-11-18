{
  lib,
  pkgs,
  package,
  settings,
}:
let
  json = pkgs.formats.json { };
  generated = json.generate "soundy-settings.json" ({ version = 1; } // settings);
  apply = pkgs.writeShellApplication {
    name = "soundy-apply-settings";
    runtimeInputs = [ pkgs.jq ];
    text = ''
      generated=$1
      dest="''${XDG_CONFIG_HOME:-$HOME/.config}/soundy/settings.json"
      mkdir -p "$(dirname "$dest")"
      if [[ -f "$dest" ]]; then
        tmp=$(mktemp "$dest.XXXXXX")
        trap 'rm -f "$tmp"' EXIT
        jq --indent 2 -s '.[0] * .[1]' "$dest" "$generated" > "$tmp"
        mv "$tmp" "$dest"
      else
        install -m600 "$generated" "$dest"
      fi
    '';
  };
  merge = "${lib.getExe apply} ${generated}";
  alsaPluginDir = pkgs.symlinkJoin {
    name = "soundy-alsa-plugins";
    paths = [
      "${pkgs.pipewire}/lib/alsa-lib"
      "${pkgs.alsa-plugins}/lib/alsa-lib"
    ];
  };
  wrapped = pkgs.symlinkJoin {
    name = "soundy";
    paths = [ package ];
    nativeBuildInputs = [ pkgs.makeWrapper ];
    postBuild = ''
      makeWrapper ${lib.getExe package} $out/bin/soundy \
        ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux "--set ALSA_PLUGIN_DIR ${alsaPluginDir}"} \
        ${lib.optionalString (settings != { }) "--run ${lib.escapeShellArg merge}"}
    '';
  };
in
{
  package = wrapped;
  inherit merge;
}
