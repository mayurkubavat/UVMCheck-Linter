#include "uvmcheck/plugins/PythonPluginManager.h"

#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <spawn.h>
#include <sys/wait.h>
#include <unistd.h>

#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/text/SourceManager.h"
#include "uvmcheck/core/ConfigManager.h"
#include "uvmcheck/core/Json.h"
#include "uvmcheck/core/Process.h"
#include "uvmcheck/rules/SweSupport.h"
#include "uvmcheck/uvm/ConfigDbGraph.h"
#include "uvmcheck/uvm/TlmTopologyGraph.h"
#include "../rules/methodology/MethodologySupport.h"

extern char** environ;

#ifndef UVMCHECK_PYTHON_SDK_DIR
#define UVMCHECK_PYTHON_SDK_DIR ""
#endif

namespace uvmcheck::plugins {

namespace fs = std::filesystem;
using namespace slang::ast;
namespace swe = uvmcheck::rules::swe;

namespace {

std::string esc(std::string_view s) {
    std::string out = "\"";
    for (char c : s) {
        switch (c) {
            case '"': out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", static_cast<unsigned>(c));
                    out += buf;
                } else {
                    out += c;
                }
        }
    }
    return out + "\"";
}

/// Writes "filename"/"line"/"column" for a location, resolved out of macro expansions.
std::string where(const slang::SourceManager* sm, slang::SourceLocation loc) {
    if (!sm || !loc.valid()) return "\"filename\": \"\", \"line\": 1, \"column\": 1";
    auto orig = sm->getFullyOriginalLoc(loc);
    return "\"filename\": " + esc(sm->getFileName(orig)) + ", \"line\": " +
           std::to_string(sm->getLineNumber(orig)) + ", \"column\": " +
           std::to_string(sm->getColumnNumber(orig));
}

const char* visibility(Visibility v) {
    switch (v) {
        case Visibility::Local: return "local";
        case Visibility::Protected: return "protected";
        default: return "public";
    }
}

const char* direction(ArgumentDirection d) {
    switch (d) {
        case ArgumentDirection::Out: return "output";
        case ArgumentDirection::InOut: return "inout";
        case ArgumentDirection::Ref: return "ref";
        default: return "input";
    }
}

std::vector<std::string> baseChain(const ClassType& cls) {
    std::vector<std::string> chain;
    const Type* curr = cls.getBaseClass();
    for (int guard = 0; curr && curr->isClass() && guard < 64; ++guard) {
        const auto& ct = curr->getCanonicalType().as<ClassType>();
        chain.emplace_back(ct.name);
        curr = ct.getBaseClass();
    }
    return chain;
}

void writeMethod(std::ostream& out, const slang::SourceManager* sm, const SubroutineSymbol& sub) {
    out << "        {\"name\": " << esc(sub.name) << ", \"kind\": "
        << (sub.subroutineKind == SubroutineKind::Task ? "\"task\"" : "\"function\"")
        << ", \"is_virtual\": " << (sub.isVirtual() ? "true" : "false")
        << ", \"is_static\": " << (sub.flags.has(MethodFlags::Static) ? "true" : "false")
        << ", \"visibility\": \"" << visibility(sub.visibility) << "\", "
        << "\"return_type\": " << esc(sub.getReturnType().toString()) << ", " << where(sm, sub.location)
        << ",\n         \"arguments\": [";
    bool first = true;
    for (const auto* arg : sub.getArguments()) {
        out << (first ? "" : ", ") << "{\"name\": " << esc(arg->name) << ", \"type\": "
            << esc(arg->getType().toString()) << ", \"direction\": \"" << direction(arg->direction) << "\"}";
        first = false;
    }
    out << "],\n         \"calls\": [";
    first = true;
    swe::forEachCall(sub.getBody(), [&](const CallExpression& call) {
        if (call.isSystemCall()) return;
        out << (first ? "\n" : ",\n") << "           {\"name\": " << esc(call.getSubroutineName())
            << ", \"receiver\": " << esc(swe::receiverText(call)) << ", \"declared_in\": "
            << esc(rules::meth::declaringClassName(call)) << ", " << where(sm, swe::locOf(call))
            << ", \"arguments\": [";
        bool firstArg = true;
        for (const auto* a : call.arguments()) {
            out << (firstArg ? "" : ", ") << esc(a ? swe::exprText(*a) : "");
            firstArg = false;
        }
        out << "]}";
        first = false;
    });
    out << "]}";
}

void writeClass(std::ostream& out, const slang::SourceManager* sm, const ClassType& cls,
                const uvm::UvmModel& model) {
    auto role = swe::classRole(cls);
    auto chain = baseChain(cls);
    bool registered = false;
    for (const auto& m : cls.members()) {
        if (m.name == "type_id") registered = true;
    }
    out << "    {\"name\": " << esc(cls.name) << ", \"base_class\": "
        << esc(chain.empty() ? "" : chain.front()) << ", \"bases\": [";
    for (size_t i = 0; i < chain.size(); ++i) out << (i ? ", " : "") << esc(chain[i]);
    out << "], \"role\": " << esc(swe::roleName(role)) << ", \"uvm_base\": " << esc(swe::uvmBaseName(cls))
        << ", \"factory_registered\": " << (registered ? "true" : "false")
        << ", \"is_component\": " << (model.isUvmComponent(cls) ? "true" : "false")
        << ", \"is_object\": " << (model.isUvmObject(cls) ? "true" : "false")
        << ", \"is_sequence\": " << (model.isUvmSequence(cls) ? "true" : "false")
        << ", \"is_sequence_item\": " << (model.isUvmSequenceItem(cls) ? "true" : "false")
        << ", \"is_virtual\": " << (cls.isAbstract ? "true" : "false") << ", " << where(sm, cls.location)
        << ",\n      \"properties\": [";
    bool first = true;
    for (const auto& m : cls.members()) {
        if (m.kind != SymbolKind::ClassProperty) continue;
        const auto& p = m.as<ClassPropertySymbol>();
        out << (first ? "\n" : ",\n") << "        {\"name\": " << esc(p.name) << ", \"type\": "
            << esc(p.getType().toString()) << ", \"rand\": "
            << (p.randMode == RandMode::Rand ? "\"rand\"" : p.randMode == RandMode::RandC ? "\"randc\"" : "null")
            << ", \"is_static\": " << (p.lifetime == VariableLifetime::Static ? "true" : "false")
            << ", \"visibility\": \"" << visibility(p.visibility) << "\", " << where(sm, p.location) << "}";
        first = false;
    }
    out << "],\n      \"methods\": [";
    first = true;
    for (const auto* sub : swe::classMethods(cls)) {
        out << (first ? "\n" : ",\n");
        writeMethod(out, sm, *sub);
        first = false;
    }
    out << "]}";
}

/// Directory holding the uvmcheck Python package (same lookup as the config loader).
std::string sdkDir() {
    if (const char* env = std::getenv("UVMCHECK_PYTHON_SDK"); env && *env) return env;
    std::error_code ec;
    fs::path exe = fs::read_symlink("/proc/self/exe", ec);
    for (const fs::path& base : {exe.parent_path() / ".." / "python", exe.parent_path() / "python"}) {
        if (!base.empty() && fs::exists(base / "uvmcheck" / "plugin_runner.py", ec))
            return base.lexically_normal().string();
    }
    if (fs::exists(fs::path(UVMCHECK_PYTHON_SDK_DIR) / "uvmcheck" / "plugin_runner.py", ec))
        return UVMCHECK_PYTHON_SDK_DIR;
    return "";
}

core::Severity parseSeverity(const std::string& s) {
    if (s == "Error") return core::Severity::Error;
    if (s == "Info") return core::Severity::Info;
    return core::Severity::Warning;
}

} // namespace

void PythonPluginManager::writeModel(std::ostream& out, Compilation& compilation,
                                     const uvm::UvmModel& model) {
    const auto* sm = compilation.getSourceManager();
    out << "{\n  \"version\": 2,\n  \"classes\": [\n";
    bool first = true;
    for (const auto* cls : swe::userClasses(model)) {
        if (!first) out << ",\n";
        writeClass(out, sm, *cls, model);
        first = false;
    }

    out << "\n  ],\n  \"config_db\": [";
    uvm::ConfigDbGraph cfg(compilation);
    first = true;
    for (const auto& n : cfg.getNodes()) {
        if (sm && n.location.valid() && !rules::meth::isUserLocation(*sm, n.location)) continue;
        out << (first ? "\n" : ",\n") << "    {\"op\": \"" << (n.kind == uvm::ConfigDbNode::Kind::Set ? "set" : "get")
            << "\", \"type\": " << esc(n.typeName) << ", \"in_class\": " << esc(n.contextClass)
            << ", \"inst_name\": " << esc(n.instName) << ", \"field_name\": " << esc(n.fieldName) << ", "
            << where(sm, n.location) << "}";
        first = false;
    }

    out << "],\n  \"tlm_ports\": [";
    uvm::TlmTopologyGraph tlm(compilation, model);
    first = true;
    for (const auto& p : tlm.getPorts()) {
        out << (first ? "\n" : ",\n") << "    {\"component\": " << esc(p.componentClass) << ", \"name\": "
            << esc(p.portName) << ", \"type\": " << esc(p.typeName) << ", \"is_pull\": "
            << (p.isPullPort ? "true" : "false") << ", \"is_analysis\": " << (p.isAnalysisPort ? "true" : "false")
            << ", " << where(sm, p.location) << "}";
        first = false;
    }
    out << "],\n  \"tlm_connections\": [";
    first = true;
    for (const auto& c : tlm.getConnections()) {
        out << (first ? "\n" : ",\n") << "    {\"in_class\": " << esc(c.callerClass) << ", \"source\": "
            << esc(c.sourceExpr) << ", \"target\": " << esc(c.targetExpr) << ", " << where(sm, c.location) << "}";
        first = false;
    }

    out << "],\n  \"component_paths\": [";
    first = true;
    for (const auto& p : model.getComponentPaths()) {
        out << (first ? "" : ", ") << esc(p);
        first = false;
    }

    // Per-rule options from the project config, keyed as written (rule ID or class name).
    out << "],\n  \"rule_options\": {";
    first = true;
    const auto& pc = core::ConfigManager::instance().getProjectConfig();
    for (const auto& [key, settings] : const_cast<core::ProjectConfig&>(pc).ruleSettings()) {
        if (!settings.options.isObject()) continue;
        out << (first ? "\n" : ",\n") << "    " << esc(key) << ": {";
        bool firstOpt = true;
        for (const auto& [k, v] : settings.options.asObject()) {
            out << (firstOpt ? "" : ", ") << esc(k) << ": ";
            if (v.isBool()) out << (v.asBool() ? "true" : "false");
            else if (v.isNumber()) out << v.asNumber();
            else if (v.isString()) out << esc(v.asString());
            else out << "null";
            firstOpt = false;
        }
        out << "}";
        first = false;
    }
    out << "\n  }\n}\n";
}

bool PythonPluginManager::runPythonRules(const std::string& rulesPath, Compilation& compilation,
                                         const uvm::UvmModel& uvmModel, core::DiagnosticEngine& diags) {
    // 1. Model into a private temporary file.
    std::error_code ec;
    std::string tmpl = (fs::temp_directory_path(ec) / "uvmcheck_model_XXXXXX").string();
    std::vector<char> pathBuf(tmpl.begin(), tmpl.end());
    pathBuf.push_back('\0');
    int fd = mkstemp(pathBuf.data());
    if (fd < 0) {
        std::cerr << "[uvmcheck-plugins] Error: cannot create a temporary model file: " << std::strerror(errno) << "\n";
        return false;
    }
    close(fd);
    std::string modelPath(pathBuf.data());
    {
        std::ofstream out(modelPath);
        writeModel(out, compilation, uvmModel);
    }

    // 2. Runner, with the SDK on PYTHONPATH wherever uvmcheck is run from.
    const char* pyEnv = std::getenv("UVMCHECK_PYTHON");
    std::string python = (pyEnv && *pyEnv) ? pyEnv : "python3";
    std::vector<std::string> env;
    if (std::string sdk = sdkDir(); !sdk.empty()) {
        const char* old = std::getenv("PYTHONPATH");
        env.push_back("PYTHONPATH=" + sdk + (old && *old ? std::string(":") + old : ""));
    }
    std::string output;
    int rc = core::runCapture({python, "-m", "uvmcheck.plugin_runner", "--rules", rulesPath, "--model", modelPath},
                        env, output);
    fs::remove(modelPath, ec);
    // Exit code 2: some rule raised (the traceback is on stderr); the others still reported.
    if (rc == 2) {
        std::cerr << "[uvmcheck-plugins] Some Python rules failed; their findings are missing\n";
    } else if (rc != 0) {
        std::cerr << "[uvmcheck-plugins] Python rule runner failed (exit code " << rc << ")\n";
        return false;
    }

    // 3. Findings.
    core::json::ParseError pe;
    auto results = core::json::parse(output, pe);
    if (!results || !results->isArray()) {
        std::cerr << "[uvmcheck-plugins] Python rule runner printed invalid JSON (" << pe.message << ")\n";
        return false;
    }
    auto str = [](const core::json::Value& o, std::string_view k) {
        const auto* v = o.find(k);
        return v && v->isString() ? v->asString() : std::string();
    };
    auto num = [](const core::json::Value& o, std::string_view k) {
        const auto* v = o.find(k);
        return v && v->isNumber() && v->asNumber() >= 1 ? static_cast<size_t>(v->asNumber()) : size_t(1);
    };
    for (const auto& r : results->asArray()) {
        if (!r.isObject()) continue;
        std::string ruleId = str(r, "rule_id");
        if (ruleId.empty()) continue;
        std::string ruleName = str(r, "rule_name");
        diags.addDirectViolation(ruleId, ruleName.empty() ? ruleId : ruleName, str(r, "message"),
                                 parseSeverity(str(r, "severity")), str(r, "filename"), num(r, "line"),
                                 num(r, "column"));
    }
    return true;
}

} // namespace uvmcheck::plugins
