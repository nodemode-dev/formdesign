const LOGO = '/formdesign-logo.png';

export function brandMark(size: number, label = ''): string {
  return `<img class="cv-mark" src="${LOGO}" width="${size}" height="${size}" alt="${label}" draggable="false">`;
}

export function brandSvg(): string {
  return `<img src="${LOGO}" width="64" height="64" alt="" draggable="false">`;
}
