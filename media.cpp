#include "veyra/engine/EngineController.h"
#include "veyra/remoteplay/ProfileStore.h"
#include "veyra/remoteplay/ControllerInput.h"
#include "veyra/remoteplay/Discovery.h"
#include "veyra/source/RemotePlaySource.h"
#include <format>
#include "veyra/Log.h"
#include <chiaki/regist.h>
#include <chiaki/log.h>
#include <atomic>
using namespace veyra;
#define API extern "C" __declspec(dllexport)
struct Player {
    HWND parent, video;
    engine::EngineController engine;
    engine::EnhancementSettings quality;
    remoteplay::ControllerInput pad;
    bool padStarted=false;
    explicit Player(HWND window):parent(window),video(CreateWindowExW(0,L"STATIC",nullptr,
        WS_CHILD|WS_VISIBLE|WS_CLIPSIBLINGS,0,0,1,1,window,nullptr,GetModuleHandleW(nullptr),nullptr)) {}
    ~Player(){engine.stop();pad.stop();if(video)DestroyWindow(video);}
};
API Player* portal_new(HWND window,const wchar_t* logPath) noexcept {
    auto& logger=Logger::instance();
    logger.setConsoleEnabled(false);
    logger.setRotation(8ull<<20,32ull<<20);
    if(logPath)logger.openFile(logPath,true);
    try {
        // GPUI paints the parent HWND. Leave the child video rectangle to DXGI.
        SetWindowLongPtrW(window,GWL_STYLE,GetWindowLongPtrW(window,GWL_STYLE)|WS_CLIPCHILDREN);
        auto p=std::make_unique<Player>(window);
        return p->video?p.release():nullptr;
    }catch(...){return nullptr;}
}
API void portal_free(Player* p){delete p;}
// The Rust registration worker polls this Chiaki callback and owns the timeout.
struct Registration {
    ChiakiRegist handle{};
    ChiakiLog log{};
    remoteplay::NativeConnectRequest saved;
    std::array<uint8_t,6> mac{};
    std::atomic<int> result{0};
    static void completed(ChiakiRegistEvent* e,void* user) noexcept {
        auto& r=*static_cast<Registration*>(user);
        if(!e || e->type!=CHIAKI_REGIST_EVENT_TYPE_FINISHED_SUCCESS || !e->registered_host){
            r.result.store(-2);return;
        }
        auto& h=*e->registered_host;
        memcpy(r.mac.data(),h.server_mac,r.mac.size());
        memcpy(r.saved.credentials.registrationKey.data(),h.rp_regist_key,16);
        memcpy(r.saved.credentials.sessionKey.data(),h.rp_key,16);
        r.result.store(1);
    }
};
API Registration* portal_pair_start(const char* host,const unsigned char* account,unsigned pin) noexcept {
    try {
        if(!remoteplay::validHost(host)||pin>99999999||!remoteplay::initializeChiaki().ok)return nullptr;
        auto r=std::make_unique<Registration>();
        r->saved.host=host;memcpy(r->saved.credentials.accountId.data(),account,8);
        chiaki_log_init(&r->log,0,nullptr,nullptr);
        ChiakiRegistInfo info{};info.target=CHIAKI_TARGET_PS5_1;info.host=r->saved.host.c_str();info.pin=pin;
        memcpy(info.psn_account_id,account,8);
        if(chiaki_regist_start(&r->handle,&r->log,&info,Registration::completed,r.get())!=CHIAKI_ERR_SUCCESS)return nullptr;
        return r.release();
    }catch(...){return nullptr;}
}
API int portal_pair_status(Registration* r){return r->result.load();}
API bool portal_pair_finish(Registration* r,const wchar_t* profile) noexcept {
    std::unique_ptr<Registration> owner(r);
    chiaki_regist_stop(&r->handle);chiaki_regist_fini(&r->handle);
    try{
        if(!profile||r->result.load()!=1)return false;
        for(auto b:r->mac)r->saved.consoleId+=std::format("{:02x}",b);
        if(r->saved.consoleId=="000000000000")r->saved.consoleId.clear();
        return remoteplay::saveProfile(profile,r->saved);
    }catch(...){return false;}
}
API int portal_open(Player* p,const wchar_t* profile,bool wake,uint32_t bitrateKbps) noexcept {
    try {auto saved=remoteplay::loadProfile(profile);if(!saved)return -1;
        if(wake)return remoteplay::wakeLocalPs5(*saved).ok?0:-2;
        saved->video.bitrateKbps=bitrateKbps;
        if(saved->video.validate())return -4;
        source::RemotePlayConnectDesc desc;desc.request=std::move(*saved);
        if(!p->padStarted)p->padStarted=p->pad.start();
        p->engine.openRemotePlay(p->video,std::move(desc),engine::PlayerOptions::from(p->quality));return 0;
    }catch(...){return -3;}
}
API void portal_stop(Player* p){p->engine.stop();p->pad.stop();p->padStarted=false;}
API int portal_tick(Player* p,int sidebar,bool show){
    RECT r{};GetClientRect(p->parent,&r);RECT rect{};GetWindowRect(p->video,&rect);
    POINT origin{rect.left,rect.top};ScreenToClient(p->parent,&origin);
    auto x=MulDiv(sidebar,GetDpiForWindow(p->parent),96);
    auto w=std::max<LONG>(1,r.right-x),h=std::max<LONG>(1,r.bottom);
    if(origin.x!=x||origin.y||rect.right-rect.left!=w||rect.bottom-rect.top!=h)MoveWindow(p->video,x,0,w,h,TRUE);
    if(bool(IsWindowVisible(p->video))!=show)ShowWindow(p->video,show?SW_SHOWNA:SW_HIDE);
    if(p->padStarted){bool focused=GetForegroundWindow()==p->parent;p->engine.remotePlayController(p->pad.poll(focused));p->pad.feedback(p->engine.remotePlayFeedback(),focused);}
    auto state=p->engine.snapshot();
    if(state.remoteStream.state==remoteplay::SessionState::LoginPinRequired)return 7;
    return int(state.transport);
}
API bool portal_quality(Player* p,const float* values,size_t count) noexcept {
    try {
        engine::EnhancementSettings q;
        size_t i=0;
#define FIELD(group,label,field,low,high,initial,step) \
        if(i>=count || !std::isfinite(values[i]) || values[i]<low || values[i]>high)return false; \
        q.field=static_cast<decltype(q.field)>(values[i++]);
#include "quality.def"
#undef FIELD
        if(i!=count)return false;
        for(auto& layer:q.nrLayers){layer.lowLatencyPairing=q.lowLatency;layer.runtime=engine::NrRuntime::Ampere;}
        const auto& first=q.nrLayers[0];
        q.nrPolicy=first.sizePolicy;q.model=first.model;q.residual=first.residual;q.nrTemporal=first.temporal;
        if(!p->engine.requestSettings(q))return false;
        p->quality=q;return true;
    }catch(...){return false;}
}

API void portal_login(Player* p,const char* pin){p->engine.remotePlayLoginPin(pin);}


// The callback is synchronous and its strings are borrowed for that call only.
using DiscoveryCallback = void (*)(void*,const char*,bool);
API int portal_discover(DiscoveryCallback callback,void* context) noexcept {
    if(!callback)return -1;
    try {
        const auto report=remoteplay::discoverLocalPs5({});
        for(const auto& host:report.hosts)callback(context,host.host.c_str(),host.standby);
        return report.error;
    }catch(...){return -1;}
}
