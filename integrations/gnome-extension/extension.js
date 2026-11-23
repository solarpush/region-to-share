import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import Clutter from 'gi://Clutter';

import { Extension, gettext as _ } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import * as QuickSettings from 'resource:///org/gnome/shell/ui/quickSettings.js';

const DBUS_NAME = 'io.github.solarpush.RegionToShare';
const DBUS_PATH = '/io/github/solarpush/RegionToShare';

const RegionToShareIface = `
<node>
  <interface name="io.github.solarpush.RegionToShare1">
    <method name="ShowWindow"/>
    <method name="OpenSettings"/>
    <method name="StartShare">
      <arg type="b" name="native_portal" direction="in"/>
      <arg type="s" name="preset_or_region" direction="in"/>
      <arg type="b" name="virtual_cam" direction="in"/>
      <arg type="s" name="result" direction="out"/>
    </method>
    <method name="StartRecord">
      <arg type="b" name="native_portal" direction="in"/>
      <arg type="s" name="preset_or_region" direction="in"/>
      <arg type="s" name="output_path" direction="in"/>
      <arg type="b" name="also_share" direction="in"/>
      <arg type="s" name="result" direction="out"/>
    </method>
    <method name="TakeSnapshot">
      <arg type="b" name="to_clipboard" direction="in"/>
      <arg type="s" name="output_path" direction="in"/>
      <arg type="s" name="result" direction="out"/>
    </method>
    <method name="ToggleRecord">
      <arg type="b" name="is_recording" direction="out"/>
    </method>
    <method name="Pause"/>
    <method name="Resume"/>
    <method name="Stop"/>
    <method name="GetStatusJson">
      <arg type="s" name="json" direction="out"/>
    </method>
    <signal name="StateChanged">
      <arg type="s" name="new_state"/>
    </signal>
    <signal name="RecordingFinished">
      <arg type="s" name="file_path"/>
      <arg type="t" name="duration_secs"/>
      <arg type="t" name="size_bytes"/>
    </signal>
  </interface>
</node>`;

const RegionProxy = Gio.DBusProxy.makeProxyWrapper(RegionToShareIface);

function formatDuration(secs) {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

const RegionQuickToggle = GObject.registerClass(
class RegionQuickToggle extends QuickSettings.QuickMenuToggle {
    _init(extension) {
        super._init({
            title: _('Region to Share'),
            subtitle: _('Ready'),
            iconName: 'camera-video-symbolic',
            toggleMode: false,
        });

        this._ext = extension;
        this.menu.setHeader('camera-video-symbolic', _('Region to Share'), _('Share or record any screen area'));

        this._actionsSection = new PopupMenu.PopupMenuSection();
        this.menu.addMenuItem(this._actionsSection);

        this._actionsSection.addAction(_('Share Region'), () => {
            this._ext.callProxy(p => p.StartShareRemote(false, '', false, () => {}));
        });
        this._actionsSection.addAction(_('Record Region Live (MKV/MP4)'), () => {
            this._ext.callProxy(p => p.StartRecordRemote(false, '', '', true, () => {}));
        });
        this._actionsSection.addAction(_('Virtual Camera (Webcam)'), () => {
            this._ext.callProxy(p => p.StartShareRemote(false, '', true, () => {}));
        });
        this._actionsSection.addAction(_('Quick Snapshot to Clipboard'), () => {
            this._ext.callProxy(p => p.TakeSnapshotRemote(true, '', () => {}));
        });

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        this._stopItem = new PopupMenu.PopupMenuItem(_('Stop Active Session'));
        this._stopItem.connect('activate', () => {
            this._ext.callProxy(p => p.StopRemote(() => {}));
        });
        this._stopItem.visible = false;
        this.menu.addMenuItem(this._stopItem);

        this.menu.addAction(_('Open Hub / Settings'), () => {
            this._ext.callProxy(p => p.ShowWindowRemote(() => {}));
        });

        this.connect('clicked', () => {
            if (this._ext.isActive) {
                this._ext.callProxy(p => p.StopRemote(() => {}));
            } else {
                this._ext.callProxy(p => p.StartShareRemote(false, '', false, () => {}));
            }
        });
    }

    updateState(snap) {
        const st = snap.state || 'idle';
        const active = st !== 'idle' && st !== 'selecting';
        const rec = st === 'recording' || st === 'sharing+recording';

        this.checked = active;
        this._stopItem.visible = active;
        if (rec) {
            this.subtitle = `● REC ${formatDuration(snap.elapsed_seconds || 0)}`;
        } else if (active) {
            this.subtitle = `● LIVE · ${Math.round(snap.current_fps || 0)} FPS`;
        } else {
            this.subtitle = _('Ready');
        }
    }
});

const RegionIndicator = GObject.registerClass(
class RegionIndicator extends QuickSettings.SystemIndicator {
    _init(extension) {
        super._init();
        this._ext = extension;

        // Compact Top Bar pill visible whenever sharing or recording
        this._pill = new St.BoxLayout({
            style_class: 'region-to-share-pill',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });
        this._dot = new St.Label({ text: '●', style_class: 'region-to-share-dot' });
        this._label = new St.Label({ text: 'LIVE', style_class: 'region-to-share-label' });
        this._pill.add_child(this._dot);
        this._pill.add_child(this._label);
        this.add_child(this._pill);

        this._toggle = new RegionQuickToggle(extension);
        this.quickSettingsItems.push(this._toggle);
    }

    updateState(snap) {
        this._toggle.updateState(snap);
        const st = snap.state || 'idle';
        const active = st !== 'idle' && st !== 'selecting';
        const rec = st === 'recording' || st === 'sharing+recording';

        this._pill.visible = active;
        if (rec) {
            this._pill.remove_style_class_name('live');
            this._pill.add_style_class_name('recording');
            this._label.text = `REC ${formatDuration(snap.elapsed_seconds || 0)}`;
        } else if (active) {
            this._pill.remove_style_class_name('recording');
            this._pill.add_style_class_name('live');
            this._label.text = 'LIVE';
        }
    }

    destroy() {
        this._toggle.destroy();
        super.destroy();
    }
});

export default class RegionToShareExtension extends Extension {
    enable() {
        this.isActive = false;
        this._proxy = null;
        this._signalIds = [];
        this._pollSource = null;

        this._indicator = new RegionIndicator(this);
        Main.panel.statusArea.quickSettings.addExternalIndicator(this._indicator);

        this._connectProxy();
        this._pollSource = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 2, () => {
            this._refreshStatus();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _connectProxy() {
        new RegionProxy(
            Gio.DBus.session,
            DBUS_NAME,
            DBUS_PATH,
            (proxy, error) => {
                if (error) {
                    console.debug(`[RegionToShare] D-Bus connect deferred: ${error.message}`);
                    return;
                }
                this._proxy = proxy;
                this._signalIds.push(
                    proxy.connectSignal('StateChanged', () => this._refreshStatus()),
                    proxy.connectSignal('RecordingFinished', (_p, _s, [path, dur]) => {
                        Main.notify(_('Recording Saved'), `${path} (${dur}s)`);
                        this._refreshStatus();
                    }),
                );
                this._refreshStatus();
            },
            null,
            Gio.DBusProxyFlags.NONE,
        );
    }

    callProxy(fn) {
        if (this._proxy) {
            fn(this._proxy);
            return;
        }
        // D-Bus activation will spawn `region-to-share daemon` automatically
        new RegionProxy(
            Gio.DBus.session,
            DBUS_NAME,
            DBUS_PATH,
            (proxy, err) => {
                if (!err && proxy) {
                    this._proxy = proxy;
                    fn(proxy);
                }
            },
        );
    }

    _refreshStatus() {
        if (!this._proxy) return;
        this._proxy.GetStatusJsonRemote(([json], err) => {
            if (err || !json) return;
            try {
                const snap = JSON.parse(json);
                const st = snap.state || 'idle';
                this.isActive = st !== 'idle' && st !== 'selecting';
                this._indicator?.updateState(snap);
            } catch (_) {
                // ignore
            }
        });
    }

    disable() {
        if (this._pollSource) {
            GLib.Source.remove(this._pollSource);
            this._pollSource = null;
        }
        if (this._proxy) {
            for (const id of this._signalIds) {
                this._proxy.disconnectSignal(id);
            }
            this._signalIds = [];
            this._proxy = null;
        }
        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
    }
}
