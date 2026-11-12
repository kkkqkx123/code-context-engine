/**
 * Single source for semantic tone vocabulary shared by status UI.
 * Badge owns the full set; Notice and Progress reuse subsets so the
 * compiler rejects drift when a tone is renamed or removed.
 */
export type Tone = 'info' | 'success' | 'warning' | 'danger';

export type BadgeVariant = 'default' | 'active' | 'inactive' | Tone;

export type NoticeVariant = Extract<Tone, 'info' | 'warning'>;

export type ProgressVariant = 'default' | Tone;
