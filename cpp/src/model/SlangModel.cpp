#include "uvmcheck/model/SlangModel.h"

#include <optional>
#include <unordered_map>

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Compilation.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/Type.h"
#include "slang/text/SourceManager.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::model {

namespace {

namespace sa = slang::ast;

Visibility toVisibility(sa::Visibility v) {
    switch (v) {
        case sa::Visibility::Protected: return Visibility::Protected;
        case sa::Visibility::Local: return Visibility::Local;
        default: return Visibility::Public;
    }
}

TypeShape shapeOf(const sa::Type& t) {
    switch (t.getCanonicalType().kind) {
        case sa::SymbolKind::DynamicArrayType: return TypeShape::DynamicArray;
        case sa::SymbolKind::QueueType: return TypeShape::Queue;
        case sa::SymbolKind::AssociativeArrayType: return TypeShape::AssociativeArray;
        case sa::SymbolKind::FixedSizeUnpackedArrayType: return TypeShape::FixedUnpackedArray;
        case sa::SymbolKind::UnpackedStructType: return TypeShape::UnpackedStruct;
        default: return TypeShape::Other;
    }
}

bool isVif(const sa::Type& t) { return t.isVirtualInterfaceOrArray() || t.isVirtualInterface(); }

Direction toDirection(sa::ArgumentDirection d) {
    switch (d) {
        case sa::ArgumentDirection::Out: return Direction::Out;
        case sa::ArgumentDirection::InOut: return Direction::InOut;
        case sa::ArgumentDirection::Ref: return Direction::Ref;
        default: return Direction::In;
    }
}

UvmKind classify(const uvm::UvmModel& uvm, const sa::Type& t) {
    // Component and sequence checks come first: a reduced UVM (a test mock)
    // need not derive them from uvm_object.
    if (uvm.isUvmComponent(t)) {
        // Most specific first: a test is also a component.
        static constexpr std::pair<std::string_view, UvmKind> roles[] = {
            {"uvm_test", UvmKind::Test},
            {"uvm_env", UvmKind::Env},
            {"uvm_agent", UvmKind::Agent},
            {"uvm_driver", UvmKind::Driver},
            {"uvm_monitor", UvmKind::Monitor},
            {"uvm_sequencer_base", UvmKind::Sequencer},
            {"uvm_scoreboard", UvmKind::Scoreboard},
            {"uvm_subscriber", UvmKind::Subscriber},
        };
        for (const auto& [base, kind] : roles) {
            if (uvm.isDerivedFrom(t, base)) return kind;
        }
        return UvmKind::Component;
    }
    if (uvm.isUvmSequence(t)) return UvmKind::Sequence;
    if (uvm.isUvmSequenceItem(t)) return UvmKind::SequenceItem;
    return uvm.isUvmObject(t) ? UvmKind::Object : UvmKind::None;
}

class Builder {
public:
    Builder(const sa::Compilation& c, const uvm::UvmModel& u, const BuildOptions& o)
        : compilation(c), sm(c.getSourceManager()), uvm(u), options(o) {}

    Model run() {
        // Ids first, so bases and property types can point at any class:
        // the plain classes the UVM model found, then one entry per
        // parameterized class definition (uvm_driver #(REQ, RSP) and the like).
        for (const auto* ct : uvm.getAllClasses()) {
            if (ct) ids.try_emplace(ct, static_cast<ClassId>(ids.size()));
        }
        // getRoot() is not const in slang, but on an elaborated design it only
        // returns the finished root.
        collectGenerics(const_cast<sa::Compilation&>(compilation).getRoot());
        const auto firstGeneric = static_cast<ClassId>(ids.size());
        for (size_t i = 0; i < generics.size(); ++i)
            genericIds.emplace(generics[i], static_cast<ClassId>(firstGeneric + i));

        for (const auto* ct : uvm.getAllClasses()) {
            if (ct && ids.at(ct) == model.classes().size()) model.addClass(makeClass(*ct));
        }
        for (const auto* g : generics) model.addClass(makeGeneric(*g));

        // A definition nothing specialized has no type to read its base from;
        // its base was taken from the source text, so resolve it by name.
        for (ClassId id = firstGeneric; id < model.classes().size(); ++id) {
            Class& c = model.mutableClass(id);
            if (c.base == NoClass && !c.baseName.empty()) {
                if (auto b = model.findClass(c.baseName); b && *b != id) c.base = *b;
            }
        }
        return std::move(model);
    }

private:
    const sa::Compilation& compilation;
    const slang::SourceManager* sm;
    const uvm::UvmModel& uvm;
    const BuildOptions& options;
    Model model;
    std::unordered_map<const sa::ClassType*, ClassId> ids;
    std::vector<const sa::GenericClassDefSymbol*> generics;
    std::unordered_map<const sa::GenericClassDefSymbol*, ClassId> genericIds;

    // Same walk as UvmModel::indexScope: packages, $unit, nested classes.
    void collectGenerics(const sa::Scope& scope) {
        for (const auto& member : scope.members()) {
            switch (member.kind) {
                case sa::SymbolKind::GenericClassDef:
                    generics.push_back(&member.as<sa::GenericClassDefSymbol>());
                    break;
                case sa::SymbolKind::ClassType:
                    collectGenerics(member.as<sa::ClassType>());
                    break;
                case sa::SymbolKind::Package:
                    collectGenerics(member.as<sa::PackageSymbol>());
                    break;
                case sa::SymbolKind::CompilationUnit:
                    collectGenerics(member.as<sa::CompilationUnitSymbol>());
                    break;
                default:
                    break;
            }
        }
    }

    /// One existing specialization to read members from, chosen by its
    /// printed type so the choice does not depend on hash order. Never makes
    /// a new specialization: the design is already elaborated and shared.
    static const sa::ClassType* representative(const sa::GenericClassDefSymbol& g) {
        const sa::ClassType* best = nullptr;
        std::string bestName;
        for (const auto& spec : g.specializations()) {
            const auto& canon = spec.getCanonicalType();
            if (!canon.isClass()) continue;
            std::string name = canon.toString();
            if (!best || name < bestName) {
                best = &canon.as<sa::ClassType>();
                bestName = std::move(name);
            }
        }
        return best;
    }

    static std::string baseNameFromSyntax(const sa::GenericClassDefSymbol& g) {
        const auto* syntax = g.getSyntax();
        if (!syntax || syntax->kind != slang::syntax::SyntaxKind::ClassDeclaration) return {};
        const auto& decl = syntax->as<slang::syntax::ClassDeclarationSyntax>();
        if (!decl.extendsClause) return {};
        std::string text = decl.extendsClause->baseName->toString();
        if (auto hash = text.find('#'); hash != std::string::npos) text.erase(hash);
        if (auto colon = text.rfind("::"); colon != std::string::npos) text.erase(0, colon + 2);
        while (!text.empty() && (text.back() == ' ' || text.back() == '\n' || text.back() == '\t')) text.pop_back();
        while (!text.empty() && (text.front() == ' ' || text.front() == '\n' || text.front() == '\t')) text.erase(0, 1);
        return text;
    }

    /// Macro uses that start inside a class's syntax, as rules find them
    /// (the trees' top-level macro references).
    std::vector<MacroUse> macrosIn(const slang::syntax::SyntaxNode* syntax) {
        std::vector<MacroUse> out;
        if (!syntax) return out;
        if (!macroRefs) {
            macroRefs.emplace();
            for (const auto& tree : compilation.getSyntaxTrees()) {
                for (const auto& r : tree->getMacroRefs()) {
                    if (!r.syntax || !r.definition) continue;
                    macroRefs->push_back({r.syntax->sourceRange().start(), r.definition->name.valueText()});
                }
            }
        }
        const auto range = syntax->sourceRange();
        for (const auto& [at, name] : *macroRefs) {
            if (at >= range.start() && at < range.end()) out.push_back({std::string(name), loc(at)});
        }
        return out;
    }
    std::optional<std::vector<std::pair<slang::SourceLocation, std::string_view>>> macroRefs;

    Class makeGeneric(const sa::GenericClassDefSymbol& g) {
        const sa::ClassType* rep = representative(g);
        Class c = rep ? makeClass(*rep) : Class{};
        c.name = std::string(g.name);
        c.loc = loc(g.location);
        c.scope.clear();
        if (const auto* parent = g.getParentScope()) {
            const auto& sym = parent->asSymbol();
            if (sym.kind == sa::SymbolKind::Package || sym.kind == sa::SymbolKind::ClassType)
                c.scope = std::string(sym.name);
        }
        c.isParameterized = true;
        c.macros = macrosIn(g.getSyntax());
        if (!rep) {
            c.baseName = baseNameFromSyntax(g);
            if (options.isLibraryFile) c.isLibrary = options.isLibraryFile(model.fileName(c.loc));
        }
        return c;
    }

    // Source text of a range as written, without leading comments; empty
    // when it spans a macro expansion.
    std::string textOf(slang::SourceRange r) const {
        if (!sm || !r.start().valid() || r.start().buffer() != r.end().buffer() || sm->isMacroLoc(r.start()))
            return {};
        std::string_view all = sm->getSourceText(r.start().buffer());
        if (r.end().offset() > all.size() || r.end().offset() < r.start().offset()) return {};
        return std::string(all.substr(r.start().offset(), r.end().offset() - r.start().offset()));
    }

    static RootKind rootOf(const sa::Expression& e, std::string& name) {
        const sa::Expression* cur = &e;
        while (cur) {
            switch (cur->kind) {
                case sa::ExpressionKind::NamedValue: {
                    const auto& sym = cur->as<sa::NamedValueExpression>().symbol;
                    name = std::string(sym.name);
                    if (sym.kind == sa::SymbolKind::FormalArgument) return RootKind::Argument;
                    if (sym.kind == sa::SymbolKind::ClassProperty) return RootKind::Property;
                    if (sym.kind == sa::SymbolKind::Variable) return RootKind::Local;
                    return RootKind::Other;
                }
                case sa::ExpressionKind::MemberAccess:
                    cur = &cur->as<sa::MemberAccessExpression>().value();
                    break;
                case sa::ExpressionKind::ElementSelect:
                    cur = &cur->as<sa::ElementSelectExpression>().value();
                    break;
                case sa::ExpressionKind::RangeSelect:
                    cur = &cur->as<sa::RangeSelectExpression>().value();
                    break;
                default:
                    return RootKind::Other;
            }
        }
        return RootKind::Other;
    }

    // Calls, assignments, timing controls and super.new in a body, as the
    // rules' AllGood visitors see them.
    void collectBody(const sa::SubroutineSymbol& sub, Method& m) {
        struct V : sa::ASTVisitor<V, sa::VisitFlags::AllGood> {
            Builder& b;
            Method& m;
            V(Builder& b, Method& m) : b(b), m(m) {}
            void handle(const sa::CallExpression& call) {
                Call c;
                c.name = std::string(call.getSubroutineName());
                c.loc = b.loc(call.sourceRange.start());
                c.isSystem = call.subroutine.index() == 1;
                if (!c.isSystem) {
                    if (const auto* callee = std::get<0>(call.subroutine)) {
                        c.calleeDeclaredVirtual = callee->flags.has(sa::MethodFlags::Virtual);
                        c.calleeBuiltIn = callee->flags.has(sa::MethodFlags::BuiltIn);
                        if (const auto* scope = callee->getParentScope()) {
                            const auto& owner = scope->asSymbol();
                            c.calleeScope = std::string(owner.name);
                            if (owner.kind == sa::SymbolKind::ClassType) {
                                const auto& ct = owner.as<sa::ClassType>();
                                if (ct.genericClass) c.calleeScope = std::string(ct.genericClass->name);
                            }
                        }
                    }
                }
                const auto args = call.arguments();
                if (!args.empty()) {
                    c.firstArgIsNull =
                        args[0]->unwrapImplicitConversions().kind == sa::ExpressionKind::NullLiteral;
                    c.firstArgLoc = b.loc(args[0]->sourceRange.start());
                }
                if (call.syntax) {
                    // Cheap facts from the syntax; printing every call's text
                    // costs ~10% of a uvm-core lint.
                    c.viaSuper = call.syntax->getFirstToken().kind == slang::parsing::TokenKind::SuperKeyword;
                    const slang::syntax::SyntaxNode* callee = call.syntax;
                    if (callee->kind == slang::syntax::SyntaxKind::InvocationExpression)
                        callee = callee->as<slang::syntax::InvocationExpressionSyntax>().left.get();
                    if (callee->kind == slang::syntax::SyntaxKind::ScopedName) {
                        const auto& scoped = callee->as<slang::syntax::ScopedNameSyntax>();
                        if (scoped.separator.kind == slang::parsing::TokenKind::Dot) {
                            c.receiver = scoped.left->toString();
                            c.receiver.erase(0, c.receiver.find_first_not_of(" \t\r\n"));
                        }
                    }
                }
                m.calls.push_back(std::move(c));
                visitDefault(call);
            }
            void handle(const sa::AssignmentExpression& assign) {
                Assignment a;
                a.loc = b.loc(assign.sourceRange.start());
                a.isNonBlocking = assign.isNonBlocking();
                if (assign.syntax && slang::syntax::BinaryExpressionSyntax::isKind(assign.syntax->kind)) {
                    const auto& bin = assign.syntax->as<slang::syntax::BinaryExpressionSyntax>();
                    a.lhs = b.textOf(bin.left->sourceRange());
                    a.rhs = b.textOf(bin.right->sourceRange());
                } else {
                    a.lhs = b.textOf(assign.left().sourceRange);
                }
                a.rootKind = rootOf(assign.left(), a.rootName);
                m.assignments.push_back(std::move(a));
                visitDefault(assign);
            }
            void handle(const sa::NewClassExpression& expr) {
                if (expr.isSuperClass) m.callsSuperNew = true;
                visitDefault(expr);
            }
            void handle(const sa::TimedStatement& stmt) {
                m.timingControls.push_back(b.loc(stmt.sourceRange.start()));
                visitDefault(stmt);
            }
            void handle(const sa::WaitStatement& stmt) {
                m.timingControls.push_back(b.loc(stmt.sourceRange.start()));
                visitDefault(stmt);
            }
        } v(*this, m);
        const sa::Statement& body = sub.getBody();
        body.visit(v);

        // Some timing controls do not survive into the AST (an invalid body,
        // say); find them in the syntax then.
        if (m.timingControls.empty() && sub.getSyntax()) {
            struct T : slang::syntax::SyntaxVisitor<T> {
                Builder& b;
                std::vector<Loc>& out;
                T(Builder& b, std::vector<Loc>& out) : b(b), out(out) {}
                void handle(const slang::syntax::TimingControlStatementSyntax& s) {
                    out.push_back(b.loc(s.sourceRange().start()));
                }
                void handle(const slang::syntax::WaitStatementSyntax& s) {
                    out.push_back(b.loc(s.sourceRange().start()));
                }
            } t(*this, m.timingControls);
            sub.getSyntax()->visit(t);
        }

        // The top-level `super.new(...);` statement, if any.
        const sa::Statement* top = &body;
        if (top->kind == sa::StatementKind::Block) top = &top->as<sa::BlockStatement>().body;
        if (top->kind == sa::StatementKind::List) {
            int index = 0;
            for (const auto* st : top->as<sa::StatementList>().list) {
                if (!st || st->kind == sa::StatementKind::Empty) continue;
                if (st->kind == sa::StatementKind::ExpressionStatement) {
                    const auto& e = st->as<sa::ExpressionStatement>().expr;
                    if (e.kind == sa::ExpressionKind::NewClass && e.as<sa::NewClassExpression>().isSuperClass) {
                        m.superNewIndex = index;
                        m.superNewLoc = loc(st->sourceRange.start());
                        break;
                    }
                }
                ++index;
            }
        }
    }

    Loc loc(slang::SourceLocation l) {
        Loc out;
        if (!l.valid()) return out;
        out.raw = (uint64_t(l.buffer().getId()) << 32) | uint64_t(l.offset() & 0xffffffffu);
        if (!sm) return out;
        // Code from a macro (`uvm_component_utils and the like) is reported
        // where the user wrote it, as findings are.
        const auto orig = sm->getFullyOriginalLoc(l);
        out.file = model.internFile(sm->getFileName(orig));
        out.line = static_cast<uint32_t>(sm->getLineNumber(orig));
        out.column = static_cast<uint32_t>(sm->getColumnNumber(orig));
        if (sm->isMacroLoc(l)) {
            out.expFile = model.internFile(sm->getFileName(l));
            out.expLine = static_cast<uint32_t>(sm->getLineNumber(l));
        }
        return out;
    }

    ClassId idOf(const sa::Type& t) const {
        const auto& canon = t.getCanonicalType();
        if (!canon.isClass()) return NoClass;
        const auto& ct = canon.as<sa::ClassType>();
        if (auto it = ids.find(&ct); it != ids.end()) return it->second;
        // A specialization maps to its parameterized definition's entry.
        if (ct.genericClass) {
            if (auto it = genericIds.find(ct.genericClass); it != genericIds.end()) return it->second;
        }
        return NoClass;
    }

    Class makeClass(const sa::ClassType& ct) {
        Class c;
        c.name = std::string(ct.name);
        c.loc = loc(ct.location);
        if (const auto* parent = ct.getParentScope()) {
            const auto& sym = parent->asSymbol();
            if (sym.kind == sa::SymbolKind::Package || sym.kind == sa::SymbolKind::ClassType)
                c.scope = std::string(sym.name);
        }
        c.isVirtual = ct.isAbstract;
        c.isInterface = ct.isInterface;
        c.macros = macrosIn(ct.getSyntax());
        if (const auto* base = ct.getBaseClass(); base && !base->isError()) {
            const auto& canon = base->getCanonicalType();
            if (canon.isClass()) {
                const auto& bct = canon.as<sa::ClassType>();
                c.baseName = std::string(bct.genericClass ? bct.genericClass->name : bct.name);
                c.base = idOf(canon);
            }
        }
        c.uvm = classify(uvm, ct);
        if (options.isLibraryFile) c.isLibrary = options.isLibraryFile(model.fileName(c.loc));

        for (const auto& member : ct.members()) {
            if (member.kind == sa::SymbolKind::ClassProperty) {
                const auto& p = member.as<sa::ClassPropertySymbol>();
                Property prop;
                prop.name = std::string(p.name);
                prop.type = p.getType().toString();
                prop.loc = loc(p.location);
                prop.visibility = toVisibility(p.visibility);
                prop.isStatic = p.lifetime == sa::VariableLifetime::Static;
                prop.isConst = p.flags.has(sa::VariableFlags::Const);
                prop.isRand = p.randMode != sa::RandMode::None;
                prop.isVirtualInterface = isVif(p.getType());
                prop.typeClass = idOf(p.getType());
                c.properties.push_back(std::move(prop));
            } else if (member.kind == sa::SymbolKind::Subroutine ||
                       member.kind == sa::SymbolKind::MethodPrototype) {
                // An extern method is a prototype here and a body outside the
                // class; take the body, as Scope::find does. A prototype with
                // no body at all is left out.
                const sa::SubroutineSymbol* sp = nullptr;
                bool isExtern = false;
                if (member.kind == sa::SymbolKind::Subroutine) {
                    sp = &member.as<sa::SubroutineSymbol>();
                } else {
                    sp = member.as<sa::MethodPrototypeSymbol>().getSubroutine();
                    isExtern = true;
                }
                if (!sp) continue;
                const auto& s = *sp;
                Method m;
                m.isExtern = isExtern;
                m.name = std::string(s.name);
                m.loc = loc(s.location);
                m.visibility = toVisibility(s.visibility);
                m.isTask = s.subroutineKind == sa::SubroutineKind::Task;
                m.isVirtual = s.isVirtual();
                m.isPure = s.flags.has(sa::MethodFlags::Pure);
                m.isStatic = s.flags.has(sa::MethodFlags::Static);
                m.declaredVirtual = s.flags.has(sa::MethodFlags::Virtual);
                m.isBuiltIn = s.flags.has(sa::MethodFlags::BuiltIn);
                const auto& ret = s.getReturnType();
                m.returnType = ret.toString();
                m.returnsVoid = ret.isVoid();
                for (const auto* arg : s.getArguments()) {
                    Argument a;
                    a.name = std::string(arg->name);
                    a.type = arg->getType().toString();
                    a.canonicalType = arg->getType().getCanonicalType().toString();
                    a.loc = loc(arg->location);
                    a.direction = toDirection(arg->direction);
                    a.shape = shapeOf(arg->getType());
                    a.typeClass = idOf(arg->getType());
                    m.arguments.push_back(std::move(a));
                }
                for (const auto& local : s.members()) {
                    if (local.kind != sa::SymbolKind::Variable) continue;
                    const auto& var = local.as<sa::VariableSymbol>();
                    // Declared locals only: not `this` or a function's implicit return variable.
                    if (var.flags.has(sa::VariableFlags::CompilerGenerated)) continue;
                    m.locals.push_back({std::string(var.name), var.getType().toString(), loc(var.location),
                                        isVif(var.getType())});
                }
                collectBody(s, m);
                if (uvm::UvmModel::isFunctionPhase(s.name)) m.phase = PhaseKind::Function;
                else if (uvm::UvmModel::isTaskPhase(s.name)) m.phase = PhaseKind::Task;
                c.methods.push_back(std::move(m));
            } else if (member.kind == sa::SymbolKind::ConstraintBlock) {
                c.constraints.push_back({std::string(member.name), loc(member.location)});
            } else if (member.kind == sa::SymbolKind::TypeAlias) {
                c.typedefs.push_back({std::string(member.name), loc(member.location)});
            }
        }
        return c;
    }
};

} // namespace

Model buildFromSlang(const slang::ast::Compilation& compilation, const uvm::UvmModel& uvm,
                     const BuildOptions& options) {
    return Builder(compilation, uvm, options).run();
}

slang::SourceLocation toSlang(const Loc& loc) {
    if (loc.raw == 0) return slang::SourceLocation::NoLocation;
    return slang::SourceLocation(slang::BufferID(static_cast<uint32_t>(loc.raw >> 32), {}),
                                 loc.raw & 0xffffffffu);
}

} // namespace uvmcheck::model
