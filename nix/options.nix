{ lib, pkgs, ... }:
{
  options.programs.soundy = {
    enable = lib.mkEnableOption "Soundy, a lightweight native music player";

    package = lib.mkOption {
      type = lib.types.package;
    };

    settings = lib.mkOption {
      type = (pkgs.formats.json { }).type;
      default = { };
      example = {
        gapless = true;
        appearance = {
          theme = "dark";
          icons = "solar";
        };
      };
      description = ''
        Configuration written to {file}`$XDG_CONFIG_HOME/soundy/settings.json`.
        Same JSON keys as the app. Set keys are merged on switch and each launch;
        omitted keys, including resume and window bounds, are kept.
      '';
    };
  };
}
