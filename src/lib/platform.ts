/** Platform sniff for window-chrome decisions (traffic lights vs custom controls). */
export const isMac = typeof navigator !== 'undefined' && /Macintosh|Mac OS X/.test(navigator.userAgent);
