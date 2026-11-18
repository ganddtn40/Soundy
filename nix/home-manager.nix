{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.soundy;
  settings = import ./settings.nix {
    inherit pkgs lib;
    package = cfg.package;
    inherit (cfg) settings;
  };
in
{
  _class = "homeManager";
  imports = [ ./options.nix ];

  config = lib.mkIf cfg.enable {
    home.packages = [ settings.package ];

    home.activation.soundySettings = lib.mkIf (cfg.settings != { }) (
      lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        $DRY_RUN_CMD ${settings.merge}
      ''
    );
  };
}
