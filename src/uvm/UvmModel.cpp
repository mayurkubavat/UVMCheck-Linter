#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

namespace uvmcheck::uvm {

static bool globMatch(std::string_view pattern, std::string_view text) {
    size_t p = 0, t = 0;
    size_t starP = std::string_view::npos, starT = std::string_view::npos;

    while (t < text.size()) {
        if (p < pattern.size() && (pattern[p] == '?' || pattern[p] == text[t])) {
            p++;
            t++;
        } else if (p < pattern.size() && pattern[p] == '*') {
            starP = p++;
            starT = t;
        } else if (starP != std::string_view::npos) {
            p = starP + 1;
            t = ++starT;
        } else {
            return false;
        }
    }

    while (p < pattern.size() && pattern[p] == '*') {
        p++;
    }

    return p == pattern.size();
}

UvmModel::UvmModel(slang::ast::Compilation& comp)
    : compilation(comp) {
    indexScope(compilation.getRoot());
    buildComponentDag();
}

void UvmModel::indexScope(const slang::ast::Scope& scope) {
    for (const auto& member : scope.members()) {
        if (member.kind == slang::ast::SymbolKind::ClassType) {
            const auto& classType = member.as<slang::ast::ClassType>();
            discoveredClasses.push_back(&classType);
            // Recursively index nested classes
            indexScope(classType);
        } else if (member.kind == slang::ast::SymbolKind::Package) {
            indexScope(member.as<slang::ast::PackageSymbol>());
        } else if (member.kind == slang::ast::SymbolKind::CompilationUnit) {
            indexScope(member.as<slang::ast::CompilationUnitSymbol>());
        }
    }
}

bool UvmModel::isDerivedFrom(const slang::ast::Type& type, std::string_view baseClassName) const {
    if (type.isError()) return false;
    const slang::ast::Type* curr = &type.getCanonicalType();
    while (curr && curr->isClass()) {
        const auto& classType = curr->as<slang::ast::ClassType>();
        std::string_view name = classType.genericClass ? classType.genericClass->name : classType.name;
        if (name == baseClassName) {
            return true;
        }
        curr = classType.getBaseClass();
    }
    return false;
}

bool UvmModel::isFunctionPhase(std::string_view name) {
    static const std::unordered_set<std::string_view> functionPhases = {
        "build_phase",
        "connect_phase",
        "end_of_elaboration_phase",
        "start_of_simulation_phase",
        "extract_phase",
        "check_phase",
        "report_phase",
        "final_phase"
    };
    return functionPhases.count(name) > 0;
}

bool UvmModel::isTaskPhase(std::string_view name) {
    static const std::unordered_set<std::string_view> taskPhases = {
        "run_phase",
        "pre_reset_phase",
        "reset_phase",
        "post_reset_phase",
        "pre_configure_phase",
        "configure_phase",
        "post_configure_phase",
        "pre_main_phase",
        "main_phase",
        "post_main_phase",
        "pre_shutdown_phase",
        "shutdown_phase",
        "post_shutdown_phase"
    };
    return taskPhases.count(name) > 0;
}

bool UvmModel::isStandardUvmPhase(std::string_view name) {
    return isFunctionPhase(name) || isTaskPhase(name);
}

void UvmModel::buildComponentDag() {
    for (const auto* cls : discoveredClasses) {
        if (!isUvmComponent(*cls)) continue;

        for (const auto& member : cls->members()) {
            if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                if (prop.getType().isClass() && isUvmComponent(prop.getType())) {
                    std::string instName = std::string(prop.name);
                    componentPaths.push_back(instName);
                    componentPaths.push_back("uvm_test_top." + instName);
                    componentPaths.push_back("uvm_test_top.m_env." + instName);
                }
            }
        }
    }
}

bool UvmModel::matchesAnyComponentPath(std::string_view globPattern) const {
    if (globPattern.empty() || globPattern == "*") return true;

    for (const auto& path : componentPaths) {
        if (globMatch(globPattern, path)) return true;
        if (path.find(globPattern) != std::string_view::npos) return true;
    }
    return false;
}

static std::string escapeJson(std::string_view str) {
    std::string res;
    for (char c : str) {
        if (c == '"') res += "\\\"";
        else if (c == '\\') res += "\\\\";
        else if (c == '\n') res += "\\n";
        else if (c == '\r') res += "\\r";
        else if (c == '\t') res += "\\t";
        else res += c;
    }
    return res;
}

void UvmModel::dumpSemanticJson(std::ostream& out) const {
    const auto* sm = compilation.getSourceManager();

    out << "{\n";
    out << "  \"classes\": [\n";
    bool firstCls = true;
    for (const auto* cls : discoveredClasses) {
        if (!cls) continue;
        if (!firstCls) out << ",\n";
        firstCls = false;

        std::string baseClass;
        if (cls->getBaseClass()) {
            baseClass = std::string(cls->getBaseClass()->name);
        }

        std::string filename = sm ? std::string(sm->getFileName(cls->location)) : "";
        size_t line = sm ? sm->getLineNumber(cls->location) : 1;
        size_t col = sm ? sm->getColumnNumber(cls->location) : 1;

        out << "    {\n";
        out << "      \"name\": \"" << escapeJson(cls->name) << "\",\n";
        out << "      \"base_class\": \"" << escapeJson(baseClass) << "\",\n";
        out << "      \"is_component\": " << (isUvmComponent(*cls) ? "true" : "false") << ",\n";
        out << "      \"is_object\": " << (isUvmObject(*cls) ? "true" : "false") << ",\n";
        out << "      \"is_sequence\": " << (isUvmSequence(*cls) ? "true" : "false") << ",\n";
        out << "      \"is_sequence_item\": " << (isUvmSequenceItem(*cls) ? "true" : "false") << ",\n";
        out << "      \"filename\": \"" << escapeJson(filename) << "\",\n";
        out << "      \"line\": " << line << ",\n";
        out << "      \"column\": " << col << ",\n";

        // Members (properties and methods)
        out << "      \"members\": [\n";
        bool firstMem = true;
        for (const auto& member : cls->members()) {
            if (member.kind == slang::ast::SymbolKind::ClassProperty ||
                member.kind == slang::ast::SymbolKind::Subroutine) {
                if (!firstMem) out << ",\n";
                firstMem = false;

                bool isMethod = (member.kind == slang::ast::SymbolKind::Subroutine);
                std::string memType;
                if (!isMethod) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    memType = std::string(prop.getType().toString());
                }

                out << "        {\n";
                out << "          \"name\": \"" << escapeJson(member.name) << "\",\n";
                out << "          \"is_method\": " << (isMethod ? "true" : "false") << ",\n";
                out << "          \"type\": \"" << escapeJson(memType) << "\",\n";
                out << "          \"line\": " << (sm ? sm->getLineNumber(member.location) : 1) << ",\n";
                out << "          \"column\": " << (sm ? sm->getColumnNumber(member.location) : 1) << "\n";
                out << "        }";
            }
        }
        out << "\n      ]\n";
        out << "    }";
    }
    out << "\n  ],\n";

    // Component paths
    out << "  \"component_paths\": [\n";
    for (size_t i = 0; i < componentPaths.size(); ++i) {
        out << "    \"" << escapeJson(componentPaths[i]) << "\"" << (i + 1 < componentPaths.size() ? "," : "") << "\n";
    }
    out << "  ]\n";
    out << "}\n";
}

} // namespace uvmcheck::uvm
