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
#include "cadence.h"
struct Player {
    HWND parent, video, panel=nullptr;
    double reveal=0;
    int clipHeight=0, clipWidth=0;
    ULONGLONG lastPanelTick=0, lastSample=0;
    bool panelWanted=false, animations=true, panelToast=false;
    engine::EngineController engine;
    engine::EnhancementSettings quality;
    remoteplay::ControllerInput pad;
    bool padStarted=false, externalFg=false, detached=false;
    explicit Player(HWND window):parent(window),video(CreateWindowExW(0,L"STATIC",nullptr,
        WS_CHILD|WS_CLIPSIBLINGS,0,0,1,1,window,nullptr,GetModuleHandleW(nullptr),nullptr)) {}
    ~Player(){portal_cancel_measure();engine.stop();pad.stop();if(video)DestroyWindow(video);}
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
API void portal_event(const char* event){if(event)log::info("session-ui",event);}
API void portal_attach_panel(Player* p,HWND panel){
    p->panel=panel;
    SetWindowLongPtrW(panel,GWL_STYLE,WS_POPUP|WS_CLIPCHILDREN);
    SetWindowLongPtrW(panel,GWL_EXSTYLE,(GetWindowLongPtrW(panel,GWL_EXSTYLE)|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE)&~WS_EX_APPWINDOW);
    SetWindowLongPtrW(panel,GWLP_HWNDPARENT,reinterpret_cast<LONG_PTR>(p->parent));
    SetWindowTextW(panel,L"VeloraNR - Ajustes rápidos");
    SetWindowPos(panel,nullptr,0,0,0,0,SWP_FRAMECHANGED|SWP_NOSIZE|SWP_NOMOVE|SWP_NOACTIVATE|SWP_NOZORDER|SWP_HIDEWINDOW);
}
API void portal_panel(Player* p,bool wanted,int width,int height){
    if(!p->panel)return;
    const bool toast=height==48;
    if(toast!=p->panelToast){
        auto style=GetWindowLongPtrW(p->panel,GWL_EXSTYLE);
        SetWindowLongPtrW(p->panel,GWL_EXSTYLE,toast?style|WS_EX_TRANSPARENT:style&~WS_EX_TRANSPARENT);
        p->panelToast=toast;
    }
    const auto front=GetForegroundWindow();
    const bool focused=front==p->parent||front==p->video||front==p->panel;
    wanted=wanted&&focused&&!IsIconic(p->parent);
    const auto now=GetTickCount64();
    if(wanted!=p->panelWanted){
        BOOL enabled=TRUE;
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION,0,&enabled,0);
        p->animations=enabled!=FALSE;
        p->panelWanted=wanted;
        log::info("session-ui",wanted?"overlay=visible":"overlay=hidden");
    }
    double dt=std::min(0.05,(now-p->lastPanelTick)/1000.0);
    p->lastPanelTick=now;
    const double goal=wanted?1.:0.;
    p->reveal=(p->animations&&focused)?goal+(p->reveal-goal)*std::exp(-26*dt):goal;
    if(std::abs(p->reveal-goal)<0.012)p->reveal=goal;
    if(p->reveal==0){if(IsWindowVisible(p->panel))ShowWindow(p->panel,SW_HIDE);return;}
    RECT client{},old{};POINT origin{};
    GetClientRect(p->parent,&client);ClientToScreen(p->parent,&origin);
    const int dpi=GetDpiForWindow(p->parent);
    auto scale=[dpi](int v){return MulDiv(v,dpi,96);};
    int w=std::clamp<int>(scale(width),1,std::max<LONG>(1,client.right));
    int h=client.bottom;
    int clipHeight=toast?std::min<LONG>(scale(height),h):h;
    int x=origin.x, y=origin.y;
    GetWindowRect(p->panel,&old);
    bool resized=old.right-old.left!=w||old.bottom-old.top!=h;
    // Reveal by clipping the stable client surface, including across toast transitions.
    int visibleWidth=std::max(1,int(std::round(w*p->reveal)));
    if(resized||visibleWidth!=p->clipWidth||clipHeight!=p->clipHeight){
        auto region=CreateRectRgn(0,0,visibleWidth,clipHeight);
        if(region&&!SetWindowRgn(p->panel,region,FALSE))DeleteObject(region);
        p->clipWidth=visibleWidth;
        p->clipHeight=clipHeight;
    }
    bool raise=!IsWindowVisible(p->panel);
    // A rebuilt FG output may have appeared above the panel.
    if(!raise&&!toast){POINT point{x+w/2,y+scale(12)};HWND hit=WindowFromPoint(point);raise=hit&&GetAncestor(hit,GA_ROOT)!=p->panel;}
    if(raise||resized||old.left!=x||old.top!=y)
        SetWindowPos(p->panel,HWND_TOPMOST,x,y,w,h,SWP_NOACTIVATE|SWP_SHOWWINDOW|(raise?0:SWP_NOZORDER));
}
API HWND portal_capture_window(Player* p){
    return p->detached && IsWindowVisible(p->video) && !IsIconic(p->parent) ? p->video : nullptr;
}
API bool portal_preview(Player* p,const wchar_t* file) noexcept {
    try {p->engine.open(p->video,file,engine::PlayerOptions::from(p->quality));return true;}
    catch(...){return false;}
}
API void portal_stop(Player* p){p->engine.stop();p->pad.stop();p->padStarted=false;}
// Logical client coordinates work for both the DXGI child and the owned capture window.
API int portal_pointer_x(Player* p){
    HWND front=GetForegroundWindow();
    POINT pt{};RECT r{};
    if((front!=p->parent && front!=p->video && front!=p->panel) || IsIconic(p->parent) ||
       !GetCursorPos(&pt) || !ScreenToClient(p->parent,&pt) || !GetClientRect(p->parent,&r) ||
       !PtInRect(&r,pt))return -1;
    return MulDiv(pt.x,96,GetDpiForWindow(p->parent));
}
API int portal_tick(Player* p,int sidebar,bool show){
    // Windows Graphics Capture requires a top-level HWND. Keep the DXGI surface and its
    // owner, but present the video as a borderless owned window while SMFG is selected.
    if(p->externalFg != p->detached){
        ShowWindow(p->video,SW_HIDE);
        if(p->externalFg){
            SetParent(p->video,nullptr);
            SetWindowLongPtrW(p->video,GWL_STYLE,WS_POPUP|WS_CLIPSIBLINGS);
            SetWindowLongPtrW(p->video,GWLP_HWNDPARENT,reinterpret_cast<LONG_PTR>(p->parent));
            SetWindowLongPtrW(p->video,GWL_EXSTYLE,WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE);
        }else{
            SetWindowLongPtrW(p->video,GWLP_HWNDPARENT,0);
            SetWindowLongPtrW(p->video,GWL_STYLE,WS_CHILD|WS_CLIPSIBLINGS);
            SetWindowLongPtrW(p->video,GWL_EXSTYLE,0);
            SetParent(p->video,p->parent);
        }
        SetWindowPos(p->video,nullptr,0,0,0,0,SWP_FRAMECHANGED|SWP_NOSIZE|SWP_NOMOVE|SWP_NOACTIVATE|SWP_NOZORDER);
        p->detached=p->externalFg;
    }
    RECT r{};GetClientRect(p->parent,&r);RECT rect{};GetWindowRect(p->video,&rect);
    POINT origin{rect.left,rect.top};ScreenToClient(p->parent,&origin);
    auto x=MulDiv(sidebar,GetDpiForWindow(p->parent),96);
    auto w=std::max<LONG>(1,r.right-x),h=std::max<LONG>(1,r.bottom);
    if(origin.x!=x||origin.y||rect.right-rect.left!=w||rect.bottom-rect.top!=h){
        POINT at{x,0};if(p->detached)ClientToScreen(p->parent,&at);
        SetWindowPos(p->video,nullptr,at.x,at.y,w,h,SWP_NOACTIVATE|SWP_NOZORDER);
    }
    show=show&&!IsIconic(p->parent);
    if(bool(IsWindowVisible(p->video))!=show)ShowWindow(p->video,show?SW_SHOWNA:SW_HIDE);
    if(p->padStarted){auto front=GetForegroundWindow();bool focused=front==p->parent||front==p->video||front==p->panel;p->engine.remotePlayController(p->pad.poll(focused));p->pad.feedback(p->engine.remotePlayFeedback(),focused);}
    auto state=p->engine.snapshot();
    if(state.remoteStream.state==remoteplay::SessionState::LoginPinRequired)return 7;
    if(show && GetTickCount64()-p->lastSample>=1000){
        p->lastSample=GetTickCount64();
        log::info("session-sample",std::format("source={} sourceNominalFps={:.2f} receivedFps={:.2f} decodedFps={:.2f} ratesReady={} frames={} generated={} processCpuP95Ms={:.2f} presentCpuP95Ms={:.2f} lateP95Ms={:.2f} video={}x{} videoHwnd={:#x}",
          state.remotePlay?"ps5":"file",state.nominalSourceFps,state.remoteStream.receivedFps,state.remoteStream.decodedFps,state.remoteStream.ratesReady,state.frames,state.generated,state.processCpuP95Ms,state.presentCpuP95Ms,state.lateP95Ms,w,h,reinterpret_cast<uintptr_t>(p->video)));
    }
    const int effects = (int(state.applied.multiplier) << 8)
        | (int(state.nrActive) << 12) | (int(state.applying || state.nrPreparing) << 13)
        | (int(!state.backendWarning.empty()) << 14) | (int(state.srActive)<<15)
        | (int(state.applied.antiAlias.enabled)<<16) | (int(state.applied.antiAlias.method)<<17)
        | (int(state.applied.srTarget)<<19) | (int(state.fgActive)<<22);
    return int(state.transport) | effects;
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
        // Super Motion consumes the final presented image. Only one engine interpolates.
        const auto requestedMultiplier=q.multiplier;
        bool superMotion=static_cast<int>(q.frameGenerationBackend)>=9;
        if(superMotion){
            q.frameGenerationBackend=engine::EnhancementSettings{}.frameGenerationBackend;
            q.multiplier=1;
        }
        for(auto& layer:q.nrLayers){layer.lowLatencyPairing=q.lowLatency;layer.runtime=engine::NrRuntime::Ampere;}
        const auto& first=q.nrLayers[0];
        q.nrPolicy=first.sizePolicy;q.model=first.model;q.residual=first.residual;q.nrTemporal=first.temporal;
        if(!p->engine.requestSettings(q))return false;
        p->externalFg=superMotion && requestedMultiplier>1;
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
