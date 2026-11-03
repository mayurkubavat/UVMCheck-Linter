#include "uvmcheck/model/Session.h"

#include <chrono>
#include <cstdlib>
#include <exception>
#include <sstream>

#include "slang/ast/Compilation.h"
#include "slang/diagnostics/DiagnosticEngine.h"
#include "slang/driver/Driver.h"
#include "slang/util/OS.h"
#include "uvmcheck/compat/Overlay.h"
#include "uvmcheck/core/ConfigManager.h"
#include "uvmcheck/model/SlangModel.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::model {

struct Session::Impl {
    slang::driver::Driver driver;
    std::unique_ptr<slang::ast::Compilation> compilation;
    std::unique_ptr<uvm::UvmModel> uvm;
};

Session::Session() : impl_(std::make_unique<Impl>()) {}
Session::~Session() = default;

std::unique_ptr<Session> Session::load(const std::vector<std::string>& args) {
    std::unique_ptr<Session> s(new Session());
    using Clock = std::chrono::steady_clock;
    auto ms = [](Clock::time_point a, Clock::time_point b) {
        return std::chrono::duration<double, std::milli>(b - a).count();
    };
    // A library must not print: slang's messages (unreadable files, bad
    // options) go into the error text instead. The capture hook is global, so
    // loads must not run concurrently (documented on Session::load).
    std::string captured;
    auto guard = slang::OS::captureOutput([&captured](std::string_view text, bool) { captured += text; });
    auto withDetail = [&captured](std::string what) {
        while (!captured.empty() && (captured.back() == '\n' || captured.back() == ' ')) captured.pop_back();
        return captured.empty() ? what : what + ": " + captured;
    };
    try {
        auto& d = s->impl_->driver;
        d.addStandardArgs();
        compat::VendorArgs vendor;
        std::vector<const char*> argv{"uvmcheck-model"};
        for (const auto& a : args) argv.push_back(a.c_str());
        if (!d.parseCommandLine(static_cast<int>(argv.size()), argv.data())) {
            s->error_ = withDetail("invalid compile options");
            return s;
        }

        // The overrides the linter turns on by default (lang-std and friends),
        // so the model sees the same design the rules do.
        auto& project = core::ConfigManager::instance().getProjectConfig();
        compat::Overlay overlay;
        std::string error;
        if (!overlay.select(project.compat(), error)) {
            s->error_ = error;
            return s;
        }
        std::string uvmHome = project.compat().uvmHome;
        if (uvmHome.empty()) {
            if (const char* env = std::getenv("UVM_HOME")) uvmHome = env;
        }
        std::ostringstream log;
        compat::DriverContext ctx{d, project, vendor, uvmHome, log};
        if (!overlay.configureDriver(ctx) || !d.processOptions()) {
            captured += log.str();
            s->error_ = withDetail("could not apply compile options");
            return s;
        }

        auto t0 = Clock::now();
        if (!d.parseAllSources()) {
            s->error_ = withDetail("could not read the sources");
            return s;
        }
        auto t1 = Clock::now();
        s->impl_->compilation = d.createCompilation();
        overlay.configureCompilation(*s->impl_->compilation);
        for (const auto& diag : s->impl_->compilation->getAllDiagnostics()) {
            if (diag.isError()) ++s->errorCount_;
        }
        auto t2 = Clock::now();
        s->impl_->uvm = std::make_unique<uvm::UvmModel>(*s->impl_->compilation);
        s->model_ = std::make_unique<Model>(buildFromSlang(*s->impl_->compilation, *s->impl_->uvm));
        auto t3 = Clock::now();
        s->timings_ = {ms(t0, t1), ms(t1, t2), ms(t2, t3)};
    } catch (const std::exception& e) {
        s->error_ = e.what();
    }
    if (!s->model_ && s->error_.empty()) s->error_ = "no design loaded";
    return s;
}

} // namespace uvmcheck::model
