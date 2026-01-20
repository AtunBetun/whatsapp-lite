(function () {
  const WAIT_LIMIT = 50;
  const UNREAD_EVENT = 'whatsapp-lite://unread-count';
  const NOTIFICATION_EVENT = 'whatsapp-lite://notification';

  function withTauri(callback, attempt = 0) {
    if (window.__TAURI__?.event?.emit) {
      callback(window.__TAURI__);
    } else if (attempt < WAIT_LIMIT) {
      setTimeout(() => withTauri(callback, attempt + 1), 100);
    }
  }

  function preventDragAndDrop() {
    ['dragenter', 'dragover', 'dragleave', 'drop'].forEach((eventName) => {
      window.addEventListener(
        eventName,
        (event) => {
          event.preventDefault();
          event.stopPropagation();
        },
        { capture: true }
      );
    });
  }

  function trackUnread(eventApi) {
    let lastCount = -1;

    const emitCount = (count) => {
      if (count === lastCount) return;
      lastCount = count;
      eventApi.emit(UNREAD_EVENT, { count }).catch(() => {});
    };

    const parseTitle = () => {
      const match = /^\s*\((\d+)\)/.exec(document.title || '');
      return match ? Number(match[1]) : 0;
    };

    const title = document.querySelector('title');
    if (title) {
      const observer = new MutationObserver(() => emitCount(parseTitle()));
      observer.observe(title, { childList: true });
      emitCount(parseTitle());
    } else {
      emitCount(parseTitle());
    }

    const favicon = document.querySelector('link[rel~="icon"]');
    if (favicon) {
      const iconObserver = new MutationObserver(() => {
        if (lastCount <= 0 && /unread/i.test(favicon.href)) {
          emitCount(1);
        }
      });
      iconObserver.observe(favicon, { attributes: true, attributeFilter: ['href'] });
    }
  }

  function proxyNotifications(eventApi) {
    if (typeof window.Notification !== 'function') {
      return;
    }

    const NativeNotification = window.Notification;
    const proxied = new Proxy(NativeNotification, {
      construct(target, args) {
        const [title, options = {}] = args;
        eventApi
          .emit(NOTIFICATION_EVENT, {
            title: String(title ?? ''),
            options: { body: options.body ?? null },
          })
          .catch(() => {});
        return Reflect.construct(target, args);
      },
      get(target, prop, receiver) {
        if (prop === 'requestPermission') {
          return target.requestPermission.bind(target);
        }
        return Reflect.get(target, prop, receiver);
      },
      set(target, prop, value, receiver) {
        if (prop === 'permission') {
          target.permission = value;
          return true;
        }
        return Reflect.set(target, prop, value, receiver);
      },
    });

    Object.setPrototypeOf(proxied, NativeNotification);
    window.Notification = proxied;
  }

  withTauri(({ event }) => {
    preventDragAndDrop();
    trackUnread(event);
    proxyNotifications(event);
  });
})();
