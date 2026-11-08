#include "uvmcheck/reporting/TestbenchMap.h"

#include "uvmcheck/rules/SweSupport.h"
#include "uvmcheck/uvm/ConfigDbGraph.h"
#include "uvmcheck/uvm/TlmTopologyGraph.h"
#include "uvmcheck/uvm/UvmModel.h"

#include "slang/ast/Compilation.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/syntax/SyntaxNode.h"
#include "slang/text/SourceManager.h"

#include <algorithm>
#include <filesystem>
#include <fstream>
#include <map>
#include <set>
#include <sstream>

namespace uvmcheck::reporting {

namespace fs = std::filesystem;
using namespace slang::ast;
namespace swe = rules::swe;

namespace {

struct Where {
    std::string file;
    size_t line = 0;
};

struct Node {
    std::string name;
    std::string role;
    std::string file; // canonical
    std::string shownFile;
    size_t firstLine = 0, lastLine = 0;
    std::vector<std::pair<std::string, std::string>> children; // instance name, class name
    std::vector<const uvm::TlmPortDecl*> ports;
    std::set<std::string> unconnected;
    std::vector<const uvm::TlmConnectionRecord*> connections;
    std::vector<const uvm::ConfigDbNode*> config;
    std::vector<const core::Violation*> findings;
    bool contained = false;
};

std::string canonical(std::string_view p) {
    std::error_code ec;
    fs::path c = fs::weakly_canonical(fs::absolute(fs::path(p), ec), ec);
    return c.generic_string();
}

Where whereOf(const slang::SourceManager& sm, slang::SourceLocation loc) {
    if (!loc.valid()) return {};
    loc = sm.getFullyOriginalLoc(loc);
    return {std::string(sm.getFileName(loc)), sm.getLineNumber(loc)};
}

std::string html(std::string_view s) {
    std::string out;
    for (char c : s) {
        switch (c) {
            case '<': out += "&lt;"; break;
            case '>': out += "&gt;"; break;
            case '&': out += "&amp;"; break;
            case '"': out += "&quot;"; break;
            default: out += c;
        }
    }
    return out;
}

std::string js(std::string_view s) {
    std::string out = "\"";
    for (char c : s) {
        if (c == '"' || c == '\\') out += '\\';
        if (c == '\n') { out += "\\n"; continue; }
        if (static_cast<unsigned char>(c) < 0x20) continue;
        out += c;
    }
    return out + "\"";
}

struct Map {
    std::map<std::string, Node> nodes; // by class name
    std::map<std::string, std::vector<const uvm::ConfigDbNode*>> setsByField, getsByField;
    std::vector<const uvm::TlmConnectionRecord*> connections;
    std::vector<std::string> roots;
};

Map build(Compilation& comp, const uvm::UvmModel& model, const uvm::TlmTopologyGraph& tlm,
          const uvm::ConfigDbGraph& cfg, const std::vector<core::Violation>& violations) {
    Map m;
    const auto& sm = *comp.getSourceManager();
    for (const auto* cls : swe::userClasses(model)) {
        if (!model.isUvmComponent(*cls)) continue;
        Node n;
        n.name = std::string(cls->name);
        n.role = swe::roleName(swe::classRole(*cls));
        auto w = whereOf(sm, cls->location);
        n.shownFile = w.file;
        n.file = canonical(w.file);
        n.firstLine = w.line;
        n.lastLine = w.line;
        if (const auto* syn = cls->getSyntax()) {
            n.firstLine = whereOf(sm, syn->sourceRange().start()).line;
            n.lastLine = whereOf(sm, syn->sourceRange().end()).line;
        }
        for (const auto& prop : cls->membersOfType<ClassPropertySymbol>()) {
            const Type& t = prop.getType().getCanonicalType();
            if (!t.isClass() || !model.isUvmComponent(t)) continue;
            n.children.emplace_back(std::string(prop.name), std::string(t.as<ClassType>().name));
        }
        m.nodes.emplace(n.name, std::move(n));
    }
    for (auto& [_, n] : m.nodes) {
        for (const auto& [inst, child] : n.children) {
            if (auto it = m.nodes.find(child); it != m.nodes.end() && child != n.name) it->second.contained = true;
        }
    }
    std::set<std::pair<std::string, std::string>> unconnected;
    for (const auto& p : tlm.findUnconnectedPorts()) unconnected.insert({p.componentClass, p.portName});
    for (const auto& p : tlm.getPorts()) {
        if (auto it = m.nodes.find(p.componentClass); it != m.nodes.end()) {
            it->second.ports.push_back(&p);
            if (unconnected.count({p.componentClass, p.portName})) it->second.unconnected.insert(p.portName);
        }
    }
    for (const auto& c : tlm.getConnections()) {
        m.connections.push_back(&c);
        if (auto it = m.nodes.find(c.callerClass); it != m.nodes.end()) it->second.connections.push_back(&c);
    }
    for (const auto& c : cfg.getNodes()) {
        bool user = m.nodes.count(c.contextClass) > 0;
        if (!user) {
            // Keep sets/gets from user non-component code (tests' helpers, sequences), not the library.
            auto w = whereOf(sm, c.location);
            if (w.file.empty() || w.file.find("uvm_") != std::string::npos) continue;
        }
        (c.kind == uvm::ConfigDbNode::Kind::Set ? m.setsByField : m.getsByField)[c.fieldName].push_back(&c);
        if (auto it = m.nodes.find(c.contextClass); it != m.nodes.end()) it->second.config.push_back(&c);
    }
    std::map<std::string, std::vector<Node*>> byFile;
    for (auto& [_, n] : m.nodes) byFile[n.file].push_back(&n);
    for (const auto& v : violations) {
        if (v.filePath.empty()) continue;
        auto it = byFile.find(canonical(v.filePath));
        if (it == byFile.end()) continue;
        for (auto* n : it->second) {
            if (v.line >= n->firstLine && v.line <= n->lastLine) {
                n->findings.push_back(&v);
                break;
            }
        }
    }
    for (auto& [name, n] : m.nodes) {
        if (!n.contained) m.roots.push_back(name);
    }
    std::stable_sort(m.roots.begin(), m.roots.end(), [&](const std::string& a, const std::string& b) {
        return (m.nodes[a].role == std::string("test")) > (m.nodes[b].role == std::string("test"));
    });
    return m;
}

void renderNode(std::ostream& os, Map& m, const std::string& inst, const std::string& cls,
                std::set<std::string>& path, int depth) {
    auto it = m.nodes.find(cls);
    if (it == m.nodes.end()) return;
    Node& n = it->second;
    const bool open = depth < 3;
    os << "<details" << (open ? " open" : "") << "><summary><span class=\"inst\">" << html(inst) << "</span> "
       << "<span class=\"cls\">" << html(n.name) << "</span> <span class=\"role role-" << html(n.role) << "\">"
       << html(n.role) << "</span>";
    if (!n.findings.empty()) os << " <span class=\"badge\">" << n.findings.size() << " finding" << (n.findings.size() == 1 ? "" : "s") << "</span>";
    if (!n.unconnected.empty()) os << " <span class=\"badge warn\">" << n.unconnected.size() << " unconnected</span>";
    os << "</summary><div class=\"body\">";
    os << "<div class=\"where\">" << html(n.shownFile) << ":" << n.firstLine << "</div>";
    if (!n.ports.empty()) {
        os << "<div class=\"sec\">Ports</div><ul>";
        for (const auto* p : n.ports) {
            os << "<li><code>" << html(p->portName) << "</code> <span class=\"muted\">" << html(p->typeName) << "</span>"
               << (n.unconnected.count(p->portName) ? " <span class=\"badge warn\">never connected</span>" : "") << "</li>";
        }
        os << "</ul>";
    }
    if (!n.connections.empty()) {
        os << "<div class=\"sec\">Connects</div><ul>";
        for (const auto* c : n.connections) {
            os << "<li><code>" << html(c->sourceExpr) << "</code> &rarr; <code>" << html(c->targetExpr) << "</code></li>";
        }
        os << "</ul>";
    }
    if (!n.config.empty()) {
        os << "<div class=\"sec\">config_db</div><ul>";
        for (const auto* c : n.config) {
            bool set = c->kind == uvm::ConfigDbNode::Kind::Set;
            bool matched = set ? m.getsByField.count(c->fieldName) : m.setsByField.count(c->fieldName);
            os << "<li>" << (set ? "set " : "get ") << "<code>" << html(c->fieldName) << "</code> <span class=\"muted\">"
               << html(c->typeName) << (set && !c->instName.empty() ? " for " + html(c->instName) : "") << "</span>"
               << (matched ? "" : set ? " <span class=\"badge warn\">never read</span>" : " <span class=\"badge warn\">no set()</span>")
               << "</li>";
        }
        os << "</ul>";
    }
    if (!n.findings.empty()) {
        os << "<details class=\"fd\"><summary class=\"sec\">Findings (" << n.findings.size() << ")</summary><ul class=\"findings\">";
        for (const auto* v : n.findings) {
            os << "<li><span class=\"sev sev-" << core::severityToString(v->severity) << "\">" << v->ruleId
               << "</span> line " << v->line << ": " << html(v->message) << "</li>";
        }
        os << "</ul></details>";
    }
    if (!n.children.empty()) {
        os << "<div class=\"sec\">Children</div>";
        for (const auto& [childInst, childCls] : n.children) {
            if (path.count(childCls)) continue; // recursive structure: show once
            path.insert(childCls);
            renderNode(os, m, childInst, childCls, path, depth + 1);
            path.erase(childCls);
        }
    }
    os << "</div></details>\n";
}

bool writeHtml(std::ostream& os, Map& m) {
    size_t findings = 0, unconnected = 0, orphanGets = 0, unreadSets = 0;
    for (const auto& [_, n] : m.nodes) {
        findings += n.findings.size();
        unconnected += n.unconnected.size();
    }
    for (const auto& [f, gets] : m.getsByField) orphanGets += m.setsByField.count(f) ? 0 : gets.size();
    for (const auto& [f, sets] : m.setsByField) unreadSets += m.getsByField.count(f) ? 0 : sets.size();

    os << R"HTML(<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Testbench map</title>
<style>
:root{--bg:#fbfaf8;--fg:#1f2328;--muted:#6a6f76;--line:#e3e1dc;--card:#fff;--accent:#2f6fb2;--warn:#b25a00;--err:#b42318}
@media (prefers-color-scheme: dark){:root:not([data-theme="light"]){--bg:#16181b;--fg:#e6e6e3;--muted:#9aa0a6;--line:#2c3036;--card:#1d2024;--accent:#7fb0e8;--warn:#f0a35a;--err:#f2867c}}
body{background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,-apple-system,Segoe UI,sans-serif;margin:0;padding:24px 16px}
main{max-width:1100px;margin:0 auto}h1{font-size:22px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 8px}
.stats{color:var(--muted);margin-bottom:16px}details.fd{border:0;margin:0;padding:0}details{border-left:2px solid var(--line);margin:4px 0 4px 6px;padding-left:10px}
summary{cursor:pointer;padding:3px 0}.inst{font-weight:600}.cls{color:var(--muted)}.body{padding:2px 0 6px 4px}
.role{font-size:11px;border:1px solid var(--line);border-radius:10px;padding:0 7px;color:var(--accent)}
.badge{font-size:11px;border-radius:10px;padding:0 7px;background:var(--line)}.badge.warn{color:var(--warn)}
.sec{font-size:11px;text-transform:uppercase;letter-spacing:.04em;color:var(--muted);margin-top:6px}
ul{margin:2px 0;padding-left:18px}.muted,.where{color:var(--muted);font-size:12px}code{font-size:12px}
table{border-collapse:collapse;width:100%;background:var(--card)}td,th{border-bottom:1px solid var(--line);padding:5px 8px;text-align:left;font-size:13px;vertical-align:top}
.sev{font-weight:600;font-size:12px}.sev-ERROR{color:var(--err)}.sev-WARNING{color:var(--warn)}.wrap{overflow-x:auto}
</style></head><body><main>
<h1>Testbench map</h1>
)HTML";
    auto count = [](size_t n, const char* one, const char* many) { return std::to_string(n) + " " + (n == 1 ? one : many); };
    os << "<div class=\"stats\">" << count(m.nodes.size(), "component class", "component classes") << " &middot; "
       << count(m.connections.size(), "TLM connection", "TLM connections") << " &middot; "
       << count(unconnected, "unconnected port", "unconnected ports") << " &middot; "
       << count(orphanGets, "config_db get without a set", "config_db gets without a set") << " &middot; "
       << count(unreadSets, "set never read", "sets never read") << " &middot; "
       << count(findings, "finding inside components", "findings inside components") << "</div>\n<h2>Component tree</h2>\n";
    for (const auto& r : m.roots) {
        std::set<std::string> path{r};
        renderNode(os, m, m.nodes[r].role == std::string("test") ? "uvm_test_top" : r, r, path, 0);
    }
    os << "<h2>config_db flows</h2><div class=\"wrap\"><table><tr><th>Field</th><th>set() in</th><th>get() in</th></tr>\n";
    std::set<std::string> fields;
    for (const auto& [f, _] : m.setsByField) fields.insert(f);
    for (const auto& [f, _] : m.getsByField) fields.insert(f);
    for (const auto& f : fields) {
        auto list = [&](const std::map<std::string, std::vector<const uvm::ConfigDbNode*>>& by) {
            std::string s;
            auto it = by.find(f);
            if (it == by.end()) return std::string("<span class=\"badge warn\">none</span>");
            std::set<std::string> seen;
            for (const auto* n : it->second) {
                std::string label = n->contextClass.empty() ? "(top)" : n->contextClass;
                if (seen.insert(label).second) s += (s.empty() ? "" : ", ") + html(label);
            }
            return s;
        };
        os << "<tr><td><code>" << html(f) << "</code></td><td>" << list(m.setsByField) << "</td><td>"
           << list(m.getsByField) << "</td></tr>\n";
    }
    os << "</table></div>\n<h2>TLM connections</h2><div class=\"wrap\"><table><tr><th>In</th><th>From</th><th>To</th></tr>\n";
    for (const auto* c : m.connections) {
        os << "<tr><td>" << html(c->callerClass) << "</td><td><code>" << html(c->sourceExpr) << "</code></td><td><code>"
           << html(c->targetExpr) << "</code></td></tr>\n";
    }
    os << "</table></div>\n</main></body></html>\n";
    return true;
}

bool writeJson(std::ostream& os, Map& m) {
    os << "{\n  \"version\": 1,\n  \"roots\": [";
    for (size_t i = 0; i < m.roots.size(); ++i) os << (i ? ", " : "") << js(m.roots[i]);
    os << "],\n  \"components\": [";
    bool first = true;
    for (const auto& [name, n] : m.nodes) {
        os << (first ? "\n" : ",\n") << "    {\"name\": " << js(name) << ", \"role\": " << js(n.role)
           << ", \"file\": " << js(n.shownFile) << ", \"line\": " << n.firstLine << ", \"children\": [";
        for (size_t i = 0; i < n.children.size(); ++i)
            os << (i ? ", " : "") << "{\"instance\": " << js(n.children[i].first) << ", \"class\": " << js(n.children[i].second) << "}";
        os << "], \"ports\": [";
        for (size_t i = 0; i < n.ports.size(); ++i)
            os << (i ? ", " : "") << "{\"name\": " << js(n.ports[i]->portName) << ", \"type\": " << js(n.ports[i]->typeName)
               << ", \"connected\": " << (n.unconnected.count(n.ports[i]->portName) ? "false" : "true") << "}";
        os << "], \"findings\": [";
        for (size_t i = 0; i < n.findings.size(); ++i)
            os << (i ? ", " : "") << "{\"rule\": " << js(n.findings[i]->ruleId) << ", \"line\": " << n.findings[i]->line << "}";
        os << "]}";
        first = false;
    }
    os << "\n  ],\n  \"connections\": [";
    for (size_t i = 0; i < m.connections.size(); ++i)
        os << (i ? ",\n    " : "\n    ") << "{\"in\": " << js(m.connections[i]->callerClass) << ", \"from\": "
           << js(m.connections[i]->sourceExpr) << ", \"to\": " << js(m.connections[i]->targetExpr) << "}";
    os << "\n  ],\n  \"config_db\": [";
    first = true;
    for (const auto* by : {&m.setsByField, &m.getsByField}) {
        for (const auto& [f, list] : *by) {
            for (const auto* n : list) {
                os << (first ? "\n    " : ",\n    ") << "{\"op\": \"" << (by == &m.setsByField ? "set" : "get")
                   << "\", \"field\": " << js(f) << ", \"type\": " << js(n->typeName) << ", \"in\": " << js(n->contextClass)
                   << ", \"matched\": " << ((by == &m.setsByField ? m.getsByField : m.setsByField).count(f) ? "true" : "false") << "}";
                first = false;
            }
        }
    }
    os << "\n  ]\n}\n";
    return true;
}

} // namespace

bool writeTestbenchMap(const std::string& path, Compilation& compilation, const uvm::UvmModel& model,
                       const std::vector<core::Violation>& violations) {
    uvm::TlmTopologyGraph tlm(compilation, model);
    uvm::ConfigDbGraph cfg(compilation);
    Map m = build(compilation, model, tlm, cfg, violations);
    std::ofstream out(path);
    if (!out) return false;
    bool ok = path.size() > 5 && path.ends_with(".json") ? writeJson(out, m) : writeHtml(out, m);
    return ok && static_cast<bool>(out);
}

} // namespace uvmcheck::reporting
