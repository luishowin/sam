// Sam's window-control extension.
//
// Wayland only lets gnome-shell move another application's surfaces. This
// extension exposes the smallest interface that Sam needs, over the session
// bus. It has no UI, no input injection, no eval and no network access.

import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const IFACE = `
<node>
  <interface name="org.gnome.Shell.Extensions.SamWindow">
    <method name="List">
      <arg type="s" direction="out"/>
    </method>
    <method name="Activate">
      <arg type="u" direction="in"/>
      <arg type="b" direction="out"/>
    </method>
    <method name="Minimize">
      <arg type="u" direction="in"/>
      <arg type="b" direction="out"/>
    </method>
    <method name="Maximize">
      <arg type="u" direction="in"/>
      <arg type="b" direction="out"/>
    </method>
    <method name="Unmaximize">
      <arg type="u" direction="in"/>
      <arg type="b" direction="out"/>
    </method>
    <method name="MoveResize">
      <arg type="u" direction="in"/>
      <arg type="i" direction="in"/>
      <arg type="i" direction="in"/>
      <arg type="i" direction="in"/>
      <arg type="i" direction="in"/>
      <arg type="b" direction="out"/>
    </method>
    <method name="GetWorkArea">
      <arg type="s" direction="out"/>
    </method>
    <method name="Close">
      <arg type="u" direction="in"/>
      <arg type="b" direction="out"/>
    </method>
    <method name="ShowDesktop">
      <arg type="b" direction="out"/>
    </method>
  </interface>
</node>`;

const PATH = '/org/gnome/Shell/Extensions/SamWindow';

export default class SamWindowExtension extends Extension {
    enable() {
        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE, this);
        this._dbus.export(Gio.DBus.session, PATH);
    }

    disable() {
        if (this._dbus) {
            this._dbus.unexport();
            this._dbus = null;
        }
    }

    _metaWindows() {
        const out = [];
        for (const actor of global.get_window_actors()) {
            const win = actor.meta_window;
            if (!win)
                continue;
            if (win.get_window_type() !== Meta.WindowType.NORMAL)
                continue;
            if (win.is_skip_taskbar())
                continue;
            out.push(win);
        }
        return out;
    }

    _find(id) {
        return this._metaWindows().find(win => win.get_id() === id) ?? null;
    }

    List() {
        const windows = this._metaWindows().map(win => {
            const rect = win.get_frame_rect();
            const workspace = win.get_workspace();
            return {
                id: win.get_id(),
                title: win.get_title() ?? '',
                wm_class: win.get_wm_class() ?? '',
                wm_class_instance: win.get_wm_class_instance() ?? '',
                workspace: workspace ? workspace.index() : -1,
                monitor: win.get_monitor(),
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                focused: global.display.focus_window === win,
            };
        });
        return JSON.stringify(windows);
    }

    Activate(id) {
        const win = this._find(id);
        if (!win)
            return false;
        Main.activateWindow(win);
        return true;
    }

    Minimize(id) {
        const win = this._find(id);
        if (!win)
            return false;
        win.minimize();
        return true;
    }

    Maximize(id) {
        const win = this._find(id);
        if (!win)
            return false;
        win.maximize(Meta.MaximizeFlags.BOTH);
        return true;
    }

    Unmaximize(id) {
        const win = this._find(id);
        if (!win)
            return false;
        win.unmaximize(Meta.MaximizeFlags.BOTH);
        return true;
    }

    MoveResize(id, x, y, width, height) {
        const win = this._find(id);
        if (!win)
            return false;
        win.move_resize_frame(false, x, y, width, height);
        return true;
    }

    GetWorkArea() {
        const monitor = global.display.get_current_monitor();
        const area = global.workspace_manager
            .get_active_workspace()
            .get_work_area_for_monitor(monitor);
        return JSON.stringify({
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        });
    }

    Close(id) {
        const win = this._find(id);
        if (!win)
            return false;
        win.delete(global.get_current_time());
        return true;
    }

    ShowDesktop() {
        for (const win of this._metaWindows())
            win.minimize();
        return true;
    }
}
