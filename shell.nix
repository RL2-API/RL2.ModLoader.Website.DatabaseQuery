{ pkgs ? import <nixpkgs> {} } :
let
	stable = pkgs.buildPackages;
in
pkgs.mkShell {
	nativeBuildInputs = [ 
		stable.nodejs
		stable.turso-cli
	];
}
