#include "uvmcheck/compat/Overlay.h"

#include "slang/ast/ASTContext.h"
#include "slang/ast/Compilation.h"
#include "slang/ast/SystemSubroutine.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::compat {

namespace {

using namespace slang;
using namespace slang::ast;

/// A simulator's waveform or recording task: any number of arguments, where a
/// plain name may be a module instance or scope (as in $dumpvars), returns
/// nothing, and cannot run in a constant context. Names that do not resolve are
/// still reported by slang.
class VendorTask : public SystemSubroutine {
public:
    explicit VendorTask(std::string name) : SystemSubroutine(std::move(name), SubroutineKind::Task) {}

    const Expression& bindArgument(size_t argIndex, const ASTContext& context,
                                   const syntax::ExpressionSyntax& syntax, const Args& args) const override {
        if (syntax::NameSyntax::isKind(syntax.kind))
            return ArbitrarySymbolExpression::fromSyntax(context.getCompilation(), syntax.as<syntax::NameSyntax>(),
                                                         context);
        return SystemSubroutine::bindArgument(argIndex, context, syntax, args);
    }

    const Type& checkArguments(const ASTContext& context, const Args&, SourceRange,
                               const Expression*) const override {
        return context.getCompilation().getVoidType();
    }

    ConstantValue eval(EvalContext& context, const Args&, SourceRange range,
                       const CallExpression::SystemCallInfo&) const override {
        notConst(context, range);
        return nullptr;
    }
};

class VendorSystemTasks : public Override {
public:
    std::string_view id() const override { return "vendor-systasks"; }
    std::string_view summary() const override {
        return "Know the waveform and recording tasks of Verdi ($fsdb*), VCS ($vcdplus*), Xcelium ($shm_*, "
               "$recordvars) and Questa ($wlf*), instead of reporting them as unknown";
    }

    void configureCompilation(Compilation& compilation) override {
        static constexpr std::string_view names[] = {
            // Verdi / FSDB
            "$fsdbDumpfile", "$fsdbDumpvars", "$fsdbDumpon", "$fsdbDumpoff", "$fsdbDumpflush",
            "$fsdbDumpMDA", "$fsdbDumpSVA", "$fsdbDumpvarsByFile", "$fsdbAutoSwitchDumpfile",
            "$fsdbSwitchDumpfile", "$fsdbDumpFinish",
            // VCS VPD
            "$vcdpluson", "$vcdplusoff", "$vcdplusfile", "$vcdplusflush", "$vcdplusmemon",
            "$vcdplusmemoff", "$vcdplusautoflushon", "$vcdplusautoflushoff", "$vcdplusdeltacycleon",
            // Xcelium SHM and recording
            "$shm_open", "$shm_probe", "$shm_close", "$recordfile", "$recordvars", "$recordon",
            "$recordoff", "$recordsetup",
            // Questa WLF
            "$wlfdumpvars",
        };
        for (auto name : names) {
            if (!compilation.getSystemSubroutine(name))
                compilation.addSystemSubroutine(std::make_shared<VendorTask>(std::string(name)));
        }
    }
};

} // namespace

std::unique_ptr<Override> createVendorSystemTasks() {
    return std::make_unique<VendorSystemTasks>();
}

} // namespace uvmcheck::compat
