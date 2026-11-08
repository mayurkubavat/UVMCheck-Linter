// Prints the scopes Slang builds for a design, the reference the native
// builder's symbol table is checked against (tools/gates/sym_gate.py, builder
// phase N1). One line per declared symbol:
//
//     <scope path> <kind> <name>
//
// The path is the dotted chain of enclosing scopes (`m`, `p.c`, `$unit`,
// `m.f`; `$root` for design units). Every package, every module, interface and program definition
// (default-instantiated) and every compilation unit is listed with its
// declarations, and classes and subroutines with theirs. Generate blocks,
// statement blocks and compiler-made symbols are left out: elaboration
// (phase N4) checks those. Lines come out sorted.
//
// --hier prints the elaborated design instead (phase N4): from each top
// instance down, one line per instance, instance array, generate block and
// parameter, by hierarchical path:
//
//     <path> instance <name> : <definition>
//     <path> generate <name>              (instantiated blocks only)
//     <path> parameter <name> = <value>   (value parameters)
//
//   uvmcheck_sym_dump [--types] [--values] [--hier] [slang options: +incdir+DIR +define+X -f FILE ...] FILE...
#include <algorithm>
#include <cstdio>
#include <string>
#include <vector>

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Compilation.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/InstanceSymbols.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/ast/symbols/ParameterSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/driver/Driver.h"

using namespace slang;
using namespace slang::ast;

namespace {

std::vector<std::string> lines;

bool withTypes = false;
bool withValues = false;
bool withHier = false;
// Inside a parameterized class, types are left out: slang prints a type
// parameter by name or by value depending on which specialization it made
// first. The builder checks those types on specializations (phase N4).
int inGeneric = 0;

void put(const std::string& path, const char* kind, std::string_view name) {
    lines.push_back(path + " " + kind + " " + std::string(name));
}

// With --types, a value or typedef line ends in ` : <type>`.
// With --values, a parameter, specparam or enum value line ends in ` = <value>`.
void valued(const ConstantValue& v) {
    if (withValues && !inGeneric) lines.back() += " = " + v.toString();
}

void typed(const std::string& path, const char* kind, const Symbol& m, const Type& t) {
    put(path, kind, m.name);
    if (withTypes && !inGeneric) lines.back() += " : " + t.toString();
}

void scope(const std::string& path, const Scope& s);

std::string join(const std::string& path, std::string_view name) {
    return path == "$root" ? std::string(name) : path + "." + std::string(name);
}

void klass(const std::string& path, const Symbol& sym) {
    put(path, "class", sym.name);
    if (auto c = sym.as_if<ClassType>()) {
        scope(join(path, sym.name), *c);
    } else if (auto g = sym.as_if<GenericClassDefSymbol>()) {
        // With a parameter that has no default, the members that don't depend on one.
        auto t = g->getDefaultSpecialization(*sym.getParentScope());
        if (!t) t = &g->getInvalidSpecialization();
        if (t->isClass()) {
            ++inGeneric;
            scope(join(path, sym.name), t->getCanonicalType().as<ClassType>());
            --inGeneric;
        }
    }
}

void member(const std::string& path, const Symbol& m) {
    switch (m.kind) {
        case SymbolKind::Parameter:
            typed(path, "parameter", m, m.as<ParameterSymbol>().getType());
            valued(m.as<ParameterSymbol>().getValue());
            break;
        case SymbolKind::TypeParameter: put(path, "type_parameter", m.name); break;
        case SymbolKind::Port:
        case SymbolKind::MultiPort:
        case SymbolKind::InterfacePort:
            if (!m.name.empty()) put(path, "port", m.name);
            break;
        case SymbolKind::Net:
            if (!m.as<NetSymbol>().isImplicit) typed(path, "net", m, m.as<NetSymbol>().getType());
            break;
        case SymbolKind::Variable:
        case SymbolKind::ClassProperty: {
            auto& v = m.as<VariableSymbol>();
            if (!v.flags.has(VariableFlags::CompilerGenerated)) typed(path, "variable", m, v.getType());
            break;
        }
        case SymbolKind::TypeAlias: {
            typed(path, "typedef", m, m.as<TypeAliasType>().targetType.getType());
            break;
        }
        case SymbolKind::TransparentMember: {
            // Enum values; a class also carries its base classes' ones.
            auto& w = m.as<TransparentMemberSymbol>().wrapped;
            auto owner = w.getParentScope() ? w.getParentScope()->asSymbol().getParentScope() : nullptr;
            if (w.kind == SymbolKind::EnumValue && owner == m.getParentScope()) {
                put(path, "enum_value", w.name);
                valued(w.as<EnumValueSymbol>().getValue());
            }
            break;
        }
        case SymbolKind::Subroutine: {
            auto& s = m.as<SubroutineSymbol>();
            if (!s.getSyntax()) break; // built-in methods (randomize, srandom...)
            put(path, s.subroutineKind == SubroutineKind::Task ? "task" : "function", m.name);
            scope(join(path, m.name), s);
            break;
        }
        case SymbolKind::MethodPrototype: {
            auto& s = m.as<MethodPrototypeSymbol>();
            put(path, s.subroutineKind == SubroutineKind::Task ? "task" : "function", m.name);
            break;
        }
        case SymbolKind::FormalArgument: typed(path, "argument", m, m.as<FormalArgumentSymbol>().getType()); break;
        case SymbolKind::ClassType:
        case SymbolKind::GenericClassDef: klass(path, m); break;
        case SymbolKind::ConstraintBlock: put(path, "constraint", m.name); break;
        case SymbolKind::CovergroupType:
            // A class's embedded covergroup is an unnamed type plus a variable.
            if (!m.name.empty()) put(path, "covergroup", m.name);
            break;
        case SymbolKind::Modport: put(path, "modport", m.name); break;
        case SymbolKind::ClockingBlock:
            if (!m.name.empty()) put(path, "clocking", m.name);
            break;
        case SymbolKind::Genvar: put(path, "genvar", m.name); break;
        case SymbolKind::Sequence: put(path, "sequence", m.name); break;
        case SymbolKind::Property: put(path, "property", m.name); break;
        case SymbolKind::LetDecl: put(path, "let", m.name); break;
        case SymbolKind::NetType: put(path, "nettype", m.name); break;
        case SymbolKind::Specparam:
            put(path, "specparam", m.name);
            valued(m.as<SpecparamSymbol>().getValue());
            break;
        case SymbolKind::Instance:
        case SymbolKind::InstanceArray:
        case SymbolKind::UninstantiatedDef:
        case SymbolKind::CheckerInstance:
        case SymbolKind::PrimitiveInstance:
            if (!m.name.empty()) put(path, "instance", m.name);
            break;
        default: break;
    }
}

void scope(const std::string& path, const Scope& s) {
    for (auto& m : s.members()) member(path, m);
}

// ---- --hier: the elaborated hierarchy ---------------------------------------

void hier(const std::string& path, const Scope& s);

// The name of the instance array being walked (its elements have none).
std::string arrayName;

std::string blockName(const Symbol& b) {
    std::string n(b.name);
    if (n.empty()) n = std::string(b.as<GenerateBlockSymbol>().getExternalName());
    return n;
}

void hierMember(const std::string& path, const Symbol& m) {
    switch (m.kind) {
        case SymbolKind::Instance: {
            auto& i = m.as<InstanceSymbol>();
            std::string name(i.name);
            if (name.empty()) name = arrayName;
            lines.push_back(path + " instance " + name + " : " + std::string(i.getDefinition().name));
            hier(path + "." + name, i.body);
            break;
        }
        case SymbolKind::InstanceArray: {
            auto& a = m.as<InstanceArraySymbol>();
            auto saved = arrayName;
            std::string base = a.name.empty() ? saved : std::string(a.name);
            int k = 0;
            for (auto e : a.elements) {
                int idx = a.range.left >= a.range.right ? a.range.left - k : a.range.left + k;
                ++k;
                arrayName = base + "[" + std::to_string(idx) + "]";
                hierMember(path, *e);
            }
            arrayName = saved;
            break;
        }
        case SymbolKind::GenerateBlock: {
            auto& b = m.as<GenerateBlockSymbol>();
            if (b.isUninstantiated) break;
            std::string n = blockName(b);
            lines.push_back(path + " generate " + n);
            hier(path + "." + n, b);
            break;
        }
        case SymbolKind::GenerateBlockArray: {
            auto& a = m.as<GenerateBlockArraySymbol>();
            std::string n(a.name);
            if (n.empty()) n = std::string(a.getExternalName());
            for (auto e : a.entries) {
                if (e->isUninstantiated) continue;
                std::string en = n + "[" + e->arrayIndex->toString(LiteralBase::Decimal, false) + "]";
                lines.push_back(path + " generate " + en);
                hier(path + "." + en, *e);
            }
            break;
        }
        case SymbolKind::Parameter: {
            auto& p = m.as<ParameterSymbol>();
            lines.push_back(path + " parameter " + std::string(p.name) + " = " + p.getValue().toString());
            break;
        }
        default: break;
    }
}

void hier(const std::string& path, const Scope& s) {
    for (auto& m : s.members()) hierMember(path, m);
}

} // namespace

int main(int argc, char** argv) {
    // --types: also print each value's and typedef's type (phase N2).
    std::vector<char*> args;
    for (int i = 0; i < argc; ++i) {
        if (std::string(argv[i]) == "--types") withTypes = true;
        else if (std::string(argv[i]) == "--values") withValues = true;
        else if (std::string(argv[i]) == "--hier") withHier = true;
        else args.push_back(argv[i]);
    }
    argc = int(args.size());
    argv = args.data();
    driver::Driver drv;
    drv.addStandardArgs();
    if (!drv.parseCommandLine(argc, argv) || !drv.processOptions() || !drv.parseAllSources()) return 2;
    auto comp = drv.createCompilation();
    auto& root = comp->getRoot();
    if (withHier) {
        for (auto top : root.topInstances) {
            lines.push_back("$root instance " + std::string(top->name) + " : " + std::string(top->getDefinition().name));
            hier(std::string(top->name), top->body);
        }
        std::sort(lines.begin(), lines.end());
        for (auto& l : lines) std::printf("%s\n", l.c_str());
        return 0;
    }
    for (auto unit : comp->getCompilationUnits()) {
        bool any = false;
        for (auto& m : unit->members()) {
            if (m.kind == SymbolKind::Package) continue;
            any = true;
            member("$unit", m);
        }
        (void)any;
    }
    for (auto p : comp->getPackages()) {
        if (p->name == "std" || !p->getSyntax()) continue;
        put("$root", "package", p->name);
        scope(std::string(p->name), *p);
    }
    for (auto d : comp->getDefinitions()) {
        auto def = d->as_if<DefinitionSymbol>();
        if (!def) continue;
        auto& body = InstanceSymbol::createDefault(*comp, *def).body;
        const char* k = def->definitionKind == DefinitionKind::Interface ? "interface"
                        : def->definitionKind == DefinitionKind::Program ? "program"
                                                                         : "module";
        put("$root", k, def->name);
        scope(std::string(def->name), body);
    }
    std::sort(lines.begin(), lines.end());
    for (auto& l : lines) std::printf("%s\n", l.c_str());
    return 0;
}
