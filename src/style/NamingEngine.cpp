#include "uvmcheck/style/NamingEngine.h"
#include "slang/ast/symbols/BlockSymbols.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/ast/symbols/ParameterSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <cctype>
#include <iostream>

namespace uvmcheck::style {

NamingEngine::NamingEngine() {
    loadDefaultRules();
}

void NamingEngine::setRule(const std::string& targetKind, const NamingTargetSpec& spec) {
    rules[targetKind] = spec;
    compiledRegexCache.erase(targetKind);
}

bool NamingEngine::hasRule(const std::string& targetKind) const {
    auto it = rules.find(targetKind);
    return it != rules.end() && it->second.enabled;
}

const NamingTargetSpec* NamingEngine::getRule(const std::string& targetKind) const {
    auto it = rules.find(targetKind);
    if (it != rules.end() && it->second.enabled) {
        return &it->second;
    }
    return nullptr;
}

bool NamingEngine::isSnakeCase(std::string_view name) {
    if (name.empty()) return false;
    for (char c : name) {
        if (std::isupper(static_cast<unsigned char>(c))) return false;
    }
    return true;
}

bool NamingEngine::isPascalCase(std::string_view name) {
    if (name.empty() || !std::isupper(static_cast<unsigned char>(name.front()))) return false;
    if (name.find('_') != std::string_view::npos) return false;
    return true;
}

bool NamingEngine::isCamelCase(std::string_view name) {
    if (name.empty() || !std::islower(static_cast<unsigned char>(name.front()))) return false;
    if (name.find('_') != std::string_view::npos) return false;
    for (char c : name) {
        if (std::isupper(static_cast<unsigned char>(c))) return true;
    }
    return true;
}

bool NamingEngine::isAllCaps(std::string_view name) {
    if (name.empty()) return false;
    for (char c : name) {
        if (std::islower(static_cast<unsigned char>(c))) return false;
    }
    return true;
}

std::string NamingEngine::toPascalCase(std::string_view name) {
    std::string result;
    bool capNext = true;
    for (char c : name) {
        if (c == '_') {
            capNext = true;
        } else if (capNext) {
            result += static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
            capNext = false;
        } else {
            result += static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        }
    }
    return result;
}

std::string NamingEngine::toSnakeCase(std::string_view name) {
    std::string result;
    for (size_t i = 0; i < name.size(); i++) {
        char c = name[i];
        if (std::isupper(static_cast<unsigned char>(c))) {
            if (i > 0 && name[i - 1] != '_') {
                result += '_';
            }
            result += static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        } else {
            result += c;
        }
    }
    return result;
}

std::string NamingEngine::toAllCaps(std::string_view name) {
    std::string result;
    for (char c : name) {
        result += static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
    }
    return result;
}

bool NamingEngine::checkPattern(const NamingTargetSpec& spec, std::string_view name) const {
    if (!spec.regexPattern.empty()) {
        auto it = compiledRegexCache.find(spec.targetKind);
        if (it == compiledRegexCache.end()) {
            try {
                compiledRegexCache[spec.targetKind] = std::regex(spec.regexPattern);
            } catch (const std::regex_error&) {
                compiledRegexCache[spec.targetKind] = std::nullopt;
            }
            it = compiledRegexCache.find(spec.targetKind);
        }

        if (it->second.has_value()) {
            if (!std::regex_match(std::string(name), *it->second)) {
                return false;
            }
        }
    }

    if (!spec.prefix.empty()) {
        if (!name.starts_with(spec.prefix)) return false;
    }

    if (!spec.suffix.empty()) {
        if (!name.ends_with(spec.suffix)) return false;
    }

    switch (spec.casing) {
        case CasingStyle::SnakeCase:
            if (!isSnakeCase(name)) return false;
            break;
        case CasingStyle::PascalCase:
            if (!isPascalCase(name)) return false;
            break;
        case CasingStyle::CamelCase:
            if (!isCamelCase(name)) return false;
            break;
        case CasingStyle::AllCaps:
            if (!isAllCaps(name)) return false;
            break;
        case CasingStyle::None:
            break;
    }

    return true;
}

bool NamingEngine::validate(const std::string& targetKind,
                            std::string_view name,
                            std::string& outViolationReason,
                            std::string& outSuggestedName) const {
    const auto* spec = getRule(targetKind);
    if (!spec || !spec->enabled) return true;

    if (checkPattern(*spec, name)) return true;

    // Build diagnostic explanation and suggested fix
    std::string reason = targetKind + " identifier '" + std::string(name) + "' does not satisfy naming convention: ";
    std::string suggested = std::string(name);

    std::vector<std::string> requirements;
    if (!spec->prefix.empty() && !name.starts_with(spec->prefix)) {
        requirements.push_back("must begin with prefix '" + spec->prefix + "'");
        suggested = spec->prefix + suggested;
    }
    if (!spec->suffix.empty() && !name.ends_with(spec->suffix)) {
        requirements.push_back("must end with suffix '" + spec->suffix + "'");
        suggested = suggested + spec->suffix;
    }
    if (!spec->regexPattern.empty()) {
        requirements.push_back("must match pattern '" + spec->regexPattern + "'");
    }

    switch (spec->casing) {
        case CasingStyle::SnakeCase:
            if (!isSnakeCase(name)) {
                requirements.push_back("must follow snake_case");
                suggested = toSnakeCase(suggested);
            }
            break;
        case CasingStyle::PascalCase:
            if (!isPascalCase(name)) {
                requirements.push_back("must follow PascalCase");
                suggested = toPascalCase(suggested);
            }
            break;
        case CasingStyle::AllCaps:
            if (!isAllCaps(name)) {
                requirements.push_back("must follow ALL_CAPS");
                suggested = toAllCaps(suggested);
            }
            break;
        case CasingStyle::CamelCase:
        case CasingStyle::None:
            break;
    }

    for (size_t i = 0; i < requirements.size(); i++) {
        if (i > 0) reason += ", ";
        reason += requirements[i];
    }
    reason += ".";

    if (!spec->customMessage.empty()) {
        reason += " " + spec->customMessage;
    }

    outViolationReason = std::move(reason);
    outSuggestedName = std::move(suggested);
    return false;
}

void NamingEngine::loadDefaultRules() {
    rules.clear();
    compiledRegexCache.clear();

    rules["Package"] = {
        "SV-NAM-101", "Package", "^[a-z0-9_]+_pkg$", "", "_pkg", CasingStyle::SnakeCase,
        core::Severity::Warning, "Packages should end with '_pkg' for clean namespace identification.", true
    };

    rules["Interface"] = {
        "SV-NAM-102", "Interface", "^[a-z0-9_]+_if$", "", "_if", CasingStyle::SnakeCase,
        core::Severity::Warning, "Interfaces should end with '_if' suffix.", true
    };

    rules["VirtualInterface"] = {
        "SV-NAM-103", "VirtualInterface", "^[a-z0-9_]+_vif$", "", "_vif", CasingStyle::SnakeCase,
        core::Severity::Warning, "Virtual interface handles and typedefs should end with '_vif'.", true
    };

    rules["Class"] = {
        "SV-NAM-104", "Class", "", "", "", CasingStyle::PascalCase,
        core::Severity::Warning, "General classes should follow PascalCase naming conventions.", false // optional by default
    };

    rules["UvmDriver"] = {
        "UVM-NAM-101", "UvmDriver", "^[a-zA-Z0-9_]+_(driver|drv)$", "", "", CasingStyle::None,
        core::Severity::Warning, "UVM driver components must end with '_driver' or '_drv'.", true
    };

    rules["UvmMonitor"] = {
        "UVM-NAM-102", "UvmMonitor", "^[a-zA-Z0-9_]+_(monitor|mon)$", "", "", CasingStyle::None,
        core::Severity::Warning, "UVM monitor components must end with '_monitor' or '_mon'.", true
    };

    rules["UvmSequencer"] = {
        "UVM-NAM-103", "UvmSequencer", "^[a-zA-Z0-9_]+_(sequencer|sqr)$", "", "", CasingStyle::None,
        core::Severity::Warning, "UVM sequencers must end with '_sequencer' or '_sqr'.", true
    };

    rules["UvmSequence"] = {
        "UVM-NAM-104", "UvmSequence", "^[a-zA-Z0-9_]+_(seq|sequence)$", "", "", CasingStyle::None,
        core::Severity::Warning, "UVM sequence classes must end with '_seq' or '_sequence'.", true
    };

    rules["UvmSequenceItem"] = {
        "UVM-NAM-105", "UvmSequenceItem", "^[a-zA-Z0-9_]+_(item|seq_item|trans|transaction)$", "", "", CasingStyle::None,
        core::Severity::Warning, "UVM transaction/sequence items must end with '_item' or '_seq_item'.", true
    };

    rules["UvmAgent"] = {
        "UVM-NAM-106", "UvmAgent", "^[a-zA-Z0-9_]+_agent$", "", "_agent", CasingStyle::None,
        core::Severity::Warning, "UVM agents must end with '_agent' suffix.", true
    };

    rules["UvmEnv"] = {
        "UVM-NAM-107", "UvmEnv", "^[a-zA-Z0-9_]+_env$", "", "_env", CasingStyle::None,
        core::Severity::Warning, "UVM environments must end with '_env' suffix.", true
    };

    rules["UvmTest"] = {
        "UVM-NAM-108", "UvmTest", "^[a-zA-Z0-9_]+_test$", "", "_test", CasingStyle::None,
        core::Severity::Warning, "UVM test classes must end with '_test' suffix.", true
    };

    rules["UvmScoreboard"] = {
        "UVM-NAM-109", "UvmScoreboard", "^[a-zA-Z0-9_]+_(scoreboard|scb)$", "", "", CasingStyle::None,
        core::Severity::Warning, "UVM scoreboards must end with '_scoreboard' or '_scb'.", true
    };

    rules["Covergroup"] = {
        "SV-NAM-105", "Covergroup", "^[a-zA-Z0-9_]+_cg$", "", "_cg", CasingStyle::None,
        core::Severity::Warning, "Covergroup declarations must end with '_cg' suffix.", true
    };

    rules["Coverpoint"] = {
        "SV-NAM-106", "Coverpoint", "^cp_[a-zA-Z0-9_]+$", "cp_", "", CasingStyle::None,
        core::Severity::Warning, "Coverpoints should start with 'cp_' prefix.", true
    };

    rules["Cross"] = {
        "SV-NAM-107", "Cross", "^(cross|cx)_[a-zA-Z0-9_]+$", "", "", CasingStyle::None,
        core::Severity::Warning, "Cross coverages should start with 'cross_' or 'cx_' prefix.", true
    };

    rules["Assertion"] = {
        "SV-NAM-108", "Assertion", "^[a-zA-Z0-9_]+_(a|assert)$", "", "", CasingStyle::None,
        core::Severity::Warning, "Concurrent assertions should have labels ending with '_a' or '_assert'.", true
    };

    rules["Parameter"] = {
        "SV-NAM-109", "Parameter", "", "", "", CasingStyle::AllCaps,
        core::Severity::Warning, "Parameters and constants should follow ALL_CAPS conventions.", true
    };

    rules["TypedefEnum"] = {
        "SV-NAM-110", "TypedefEnum", "^[a-zA-Z0-9_]+_e$", "", "_e", CasingStyle::None,
        core::Severity::Warning, "Enum typedefs should end with '_e' suffix.", true
    };

    rules["Typedef"] = {
        "SV-NAM-111", "Typedef", "^[a-zA-Z0-9_]+_t$", "", "_t", CasingStyle::None,
        core::Severity::Warning, "User-defined typedefs should end with '_t' suffix.", true
    };

    rules["Constraint"] = {
        "SV-NAM-112", "Constraint", "^[a-zA-Z0-9_]+_c$", "", "_c", CasingStyle::None,
        core::Severity::Warning, "Class constraint blocks should end with '_c' suffix.", true
    };
}

class NamingCstVisitor : public slang::syntax::SyntaxVisitor<NamingCstVisitor> {
public:
    NamingCstVisitor(const NamingEngine& engine, const slang::SourceManager* sm, core::DiagnosticEngine& diags)
        : engine(engine), sm(sm), diags(diags) {}

    void handle(const slang::syntax::CovergroupDeclarationSyntax& cg) {
        if (sm && sm->isMacroLoc(cg.name.location())) {
            visitDefault(cg);
            return;
        }
        std::string cgName = std::string(cg.name.valueText());
        std::string reason, suggested;
        if (!engine.validate("Covergroup", cgName, reason, suggested)) {
            const auto* spec = engine.getRule("Covergroup");
            diags.addViolation(
                spec ? spec->ruleId : "SV-NAM-105",
                "CovergroupNamingConvention",
                reason,
                spec ? spec->severity : core::Severity::Warning,
                cg.name.location()
            );
        }
        visitDefault(cg);
    }

    void handle(const slang::syntax::CoverpointSyntax& cp) {
        if (cp.label) {
            if (sm && sm->isMacroLoc(cp.label->name.location())) {
                visitDefault(cp);
                return;
            }
            std::string cpName = std::string(cp.label->name.valueText());
            std::string reason, suggested;
            if (!engine.validate("Coverpoint", cpName, reason, suggested)) {
                const auto* spec = engine.getRule("Coverpoint");
                diags.addViolation(
                    spec ? spec->ruleId : "SV-NAM-106",
                    "CoverpointNamingConvention",
                    reason,
                    spec ? spec->severity : core::Severity::Warning,
                    cp.label->name.location()
                );
            }
        }
        visitDefault(cp);
    }

    void handle(const slang::syntax::CoverCrossSyntax& cross) {
        if (cross.label) {
            if (sm && sm->isMacroLoc(cross.label->name.location())) {
                visitDefault(cross);
                return;
            }
            std::string crossName = std::string(cross.label->name.valueText());
            std::string reason, suggested;
            if (!engine.validate("Cross", crossName, reason, suggested)) {
                const auto* spec = engine.getRule("Cross");
                diags.addViolation(
                    spec ? spec->ruleId : "SV-NAM-107",
                    "CrossNamingConvention",
                    reason,
                    spec ? spec->severity : core::Severity::Warning,
                    cross.label->name.location()
                );
            }
        }
        visitDefault(cross);
    }

    void handle(const slang::syntax::ConstraintDeclarationSyntax& c) {
        if (sm && sm->isMacroLoc(c.name->getFirstToken().location())) {
            visitDefault(c);
            return;
        }
        std::string cName = std::string(c.name->getLastToken().valueText());
        std::string reason, suggested;
        if (!engine.validate("Constraint", cName, reason, suggested)) {
            const auto* spec = engine.getRule("Constraint");
            diags.addViolation(
                spec ? spec->ruleId : "SV-NAM-112",
                "ConstraintNamingConvention",
                reason,
                spec ? spec->severity : core::Severity::Warning,
                c.name->getFirstToken().location()
            );
        }
        visitDefault(c);
    }

    void handle(const slang::syntax::TypedefDeclarationSyntax& td) {
        if (sm && sm->isMacroLoc(td.name.location())) {
            visitDefault(td);
            return;
        }
        std::string tdName = std::string(td.name.valueText());
        std::string targetKind = (td.type->kind == slang::syntax::SyntaxKind::EnumType) ? "TypedefEnum" : "Typedef";
        std::string reason, suggested;
        if (!engine.validate(targetKind, tdName, reason, suggested)) {
            const auto* spec = engine.getRule(targetKind);
            diags.addViolation(
                spec ? spec->ruleId : (targetKind == "TypedefEnum" ? "SV-NAM-110" : "SV-NAM-111"),
                targetKind + "NamingConvention",
                reason,
                spec ? spec->severity : core::Severity::Warning,
                td.name.location()
            );
        }
        visitDefault(td);
    }

    void handle(const slang::syntax::ModuleDeclarationSyntax& mod) {
        if (mod.kind == slang::syntax::SyntaxKind::InterfaceDeclaration) {
            if (sm && sm->isMacroLoc(mod.header->name.location())) {
                visitDefault(mod);
                return;
            }
            std::string ifName = std::string(mod.header->name.valueText());
            std::string reason, suggested;
            if (!engine.validate("Interface", ifName, reason, suggested)) {
                const auto* spec = engine.getRule("Interface");
                diags.addViolation(
                    spec ? spec->ruleId : "SV-NAM-102",
                    "InterfaceNamingConvention",
                    reason,
                    spec ? spec->severity : core::Severity::Warning,
                    mod.header->name.location()
                );
            }
        }
        visitDefault(mod);
    }

private:
    const NamingEngine& engine;
    const slang::SourceManager* sm = nullptr;
    core::DiagnosticEngine& diags;
};

void NamingEngine::run(slang::ast::Compilation& compilation,
                       const uvm::UvmModel& uvmModel,
                       core::DiagnosticEngine& diags) const {
    // 0. Package-level Inspections
    for (const auto* pkg : compilation.getPackages()) {
        // "std" is the built-in IEEE 1800 package, not user code.
        if (!pkg || pkg->name.empty() || pkg->name == "$unit" || pkg->name == "std") continue;
        std::string reason, suggested;
        if (!validate("Package", pkg->name, reason, suggested)) {
            const auto* spec = getRule("Package");
            diags.addViolation(
                spec ? spec->ruleId : "SV-NAM-101",
                "PackageNamingConvention",
                reason,
                spec ? spec->severity : core::Severity::Warning,
                pkg->location
            );
        }
    }

    // 1. AST-level Symbol Inspections
    const auto* sm = compilation.getSourceManager();
    for (const auto* cls : uvmModel.getAllClasses()) {
        if (!cls) continue;
        if (sm && sm->isMacroLoc(cls->location)) continue;
        std::string className = std::string(cls->name);

        // Determine specific UVM classification
        std::string targetKind = "Class";
        if (uvmModel.isDerivedFrom(*cls, "uvm_driver")) {
            targetKind = "UvmDriver";
        } else if (uvmModel.isDerivedFrom(*cls, "uvm_monitor")) {
            targetKind = "UvmMonitor";
        } else if (uvmModel.isDerivedFrom(*cls, "uvm_sequencer")) {
            targetKind = "UvmSequencer";
        } else if (uvmModel.isDerivedFrom(*cls, "uvm_sequence") || uvmModel.isDerivedFrom(*cls, "uvm_sequence_base")) {
            targetKind = "UvmSequence";
        } else if (uvmModel.isDerivedFrom(*cls, "uvm_sequence_item")) {
            targetKind = "UvmSequenceItem";
        } else if (uvmModel.isDerivedFrom(*cls, "uvm_agent")) {
            targetKind = "UvmAgent";
        } else if (uvmModel.isDerivedFrom(*cls, "uvm_env")) {
            targetKind = "UvmEnv";
        } else if (uvmModel.isDerivedFrom(*cls, "uvm_test")) {
            targetKind = "UvmTest";
        } else if (uvmModel.isDerivedFrom(*cls, "uvm_scoreboard")) {
            targetKind = "UvmScoreboard";
        }

        std::string reason, suggested;
        if (!validate(targetKind, className, reason, suggested)) {
            const auto* spec = getRule(targetKind);
            diags.addViolation(
                spec ? spec->ruleId : "SV-NAM-104",
                "UvmClassNamingConvention",
                reason,
                spec ? spec->severity : core::Severity::Warning,
                cls->location
            );
        }

        // Check localparams and parameters inside class
        for (const auto& member : cls->members()) {
            if (member.kind == slang::ast::SymbolKind::Parameter) {
                const auto& param = member.as<slang::ast::ParameterSymbol>();
                if (param.isLocalParam()) {
                    if (sm && sm->isMacroLoc(param.location)) continue;
                    std::string pReason, pSuggested;
                    if (!validate("Parameter", param.name, pReason, pSuggested)) {
                        const auto* spec = getRule("Parameter");
                        diags.addViolation(
                            spec ? spec->ruleId : "SV-NAM-109",
                            "ConstantNamingConvention",
                            pReason,
                            spec ? spec->severity : core::Severity::Warning,
                            param.location
                        );
                    }
                }
            }
        }
    }

    // 2. CST-level Inspections (Covergroups, Coverpoints, Crosses, Constraints)
    NamingCstVisitor cstVisitor(*this, sm, diags);
    for (const auto& tree : compilation.getSyntaxTrees()) {
        if (tree) {
            tree->root().visit(cstVisitor);
        }
    }
}

} // namespace uvmcheck::style
