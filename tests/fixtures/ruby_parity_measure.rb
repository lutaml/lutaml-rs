# Usage: ruby measure.rb <lib-dir> <cases.json>  -> JSON {name => verdict}
require "json"
$LOAD_PATH.unshift ARGV[0]
require "lutaml/model/lml/document_validator"
V = Lutaml::Model::Lml::DocumentValidator
out = {}
JSON.parse(File.read(ARGV[1])).each do |c|
  begin
    h = JSON.parse(c["raw"] || JSON.generate(c["doc"]))
    V.validate!(h, kind: c["kind"].to_sym)
    out[c["name"]] = "OK"
  rescue Lutaml::Model::Lml::RejectedError => e
    out[c["name"]] = "ERR #{e.reason}"
  rescue => e
    out[c["name"]] = "RUBYEXC #{e.class}"
  end
end
puts JSON.pretty_generate(out)
