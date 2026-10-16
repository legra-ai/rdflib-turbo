# Repository instructions

- No compliments.
- Read and respect docs/adr before changing contracts.
- Find root causes; do not add symptom fixes or silent fallbacks.
- Breaking changes are acceptable; unsupported operations fail explicitly.
- Write regression tests before fixing defects.
- Never use shell errexit (set -e or set -o errexit). Check every command's
  status explicitly at its call site. set -u and pipefail alone are fine.
- Keep the sibling pyshacl-turbo, rdflib-turbo and owlrl-turbo READMEs linked.
- Publish only through the tested ci.yml pipeline; preserve original archives.
- These three companion turbo projects are not part of the Legra platform. Do not
  describe them as a Legra RDF stack; reuse of legra-ai repositories does not imply
  platform membership. Keep this distinction in package and repository descriptions.
