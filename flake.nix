{
  inputs = { nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable"; };

  outputs = { self, nixpkgs }:
    let forAllSystems = nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed;
    in {
      # For `nix develop`:
      devShell = forAllSystems (system:
        let pkgs = (import nixpkgs) { inherit system; };
        in pkgs.mkShell {
          nativeBuildInputs = with pkgs; [ rustc cargo rustfmt clippy ];

          shellHook = ''
            export LD_LIBRARY_PATH=/run/opengl-driver/lib/:${
              pkgs.lib.makeLibraryPath
              (with pkgs; [ libGL libGLU wayland libxkbcommon ])
            }
          '';
        });
    };
}

