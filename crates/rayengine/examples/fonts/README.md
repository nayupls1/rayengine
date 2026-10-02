Bundled, unmodified fonts:

- Liberation Sans Regular: Liberation Fonts 2.1.5, from the Arch Linux
  ttf-liberation package. Upstream: https://github.com/liberationfonts/liberation-fonts
  License and copyright: LICENSE-Liberation.txt (SIL OFL 1.1).
- Press Start 2P Regular: Google Fonts, downloaded 2026-10-02 from
  https://github.com/google/fonts/tree/main/ofl/pressstart2p
  License and copyright: LICENSE-PressStart2P.txt (SIL OFL 1.1).

Keep these license files with the fonts when redistributing the example.
The example resolves paths against this directory's rayengine.toml, independent
of the process's working directory. For distribution, copy this directory and
point FontDeclarations::load at the installed manifest path.
