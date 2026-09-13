// API tipleri - backend response modelleriyle birebir.

export interface User {
	id: string;
	email: string;
	name: string;
}

export interface WorkspaceSummary {
	id: string;
	name: string;
	slug: string;
	status: string;
	role_name: string | null;
}

export interface WorkspaceDetail extends WorkspaceSummary {
	owner_id: string;
	created_at: string;
	archived_at: string | null;
}

export interface Role {
	id: string;
	name: string;
	description: string | null;
	is_system: boolean;
	permission_keys: string | null; // JSON array string
}

export interface Member {
	id: string;
	user_id: string;
	email: string;
	name: string;
	role_id: string;
	role_name: string;
	joined_at: string;
	scope_section_ids: string | null; // JSON array string
}

export interface Team {
	id: string;
	name: string;
	description: string | null;
	member_count: number;
	member_ids: string | null; // JSON array string
}

export interface Section {
	id: string;
	parent_id: string | null;
	name: string;
	sort_order: number;
	depth: number;
	archived: boolean;
	item_count: number;
	process_total: number;
	process_approved: number;
}

// --- Faz 3: Is tipleri / is kalemleri / ozellikler ---

export interface WorkType {
	id: string;
	name: string;
	description: string | null;
	is_active: boolean;
	attribute_count: number;
}

export interface AttributeDefinition {
	id: string;
	name: string;
	key: string;
	data_type: string;
	is_required: boolean;
	sort_order: number;
	is_filterable: boolean;
	default_value: string | null;
	options: string[];
}

export interface WorkItem {
	id: string;
	section_id: string;
	section_name: string;
	work_type_id: string | null;
	work_type_name: string | null;
	name: string;
	description: string | null;
	priority: string;
	status: string;
	planned_start: string | null;
	planned_end: string | null;
	created_at: string;
	process_total: number;
	process_approved: number;
}

export interface AttributeValue {
	definition_id: string;
	key: string;
	name: string;
	data_type: string;
	unit: string | null;
	is_required: boolean;
	value: unknown;
	options: string[];
}

export interface WorkItemDetail extends WorkItem {
	section_path: string[];
	attributes: AttributeValue[];
}

export const WORK_ITEM_STATUSES = [
	{ value: 'draft', label: 'Taslak' },
	{ value: 'active', label: 'Aktif' },
	{ value: 'blocked', label: 'Engelli' },
	{ value: 'completed', label: 'Tamamlandı' },
	{ value: 'cancelled', label: 'İptal' }
] as const;

export const WORK_ITEM_PRIORITIES = [
	{ value: 'low', label: 'Düşük' },
	{ value: 'medium', label: 'Normal' },
	{ value: 'high', label: 'Yüksek' },
	{ value: 'urgent', label: 'Acil' }
] as const;

export const ATTRIBUTE_DATA_TYPES = [
	{ value: 'text', label: 'Metin' },
	{ value: 'textarea', label: 'Uzun Metin' },
	{ value: 'integer', label: 'Tam Sayı' },
	{ value: 'decimal', label: 'Ondalıklı' },
	{ value: 'boolean', label: 'Evet/Hayır' },
	{ value: 'date', label: 'Tarih' },
	{ value: 'datetime', label: 'Tarih-Saat' },
	{ value: 'select', label: 'Seçim (tek)' },
	{ value: 'multiselect', label: 'Seçim (çoklu)' },
	{ value: 'email', label: 'E-posta' },
	{ value: 'phone', label: 'Telefon' },
	{ value: 'currency', label: 'Para' },
	{ value: 'percentage', label: 'Yüzde' }
] as const;

export function statusLabel(s: string): string {
	return WORK_ITEM_STATUSES.find((x) => x.value === s)?.label ?? s;
}

export function statusBadgeClass(s: string): string {
	switch (s) {
		case 'active': return 'bg-emerald-50 text-emerald-700';
		case 'completed': return 'bg-indigo-50 text-indigo-700';
		case 'blocked': return 'bg-red-50 text-red-700';
		case 'cancelled': return 'bg-slate-100 text-slate-500';
		default: return 'bg-amber-50 text-amber-700';
	}
}

export function priorityLabel(p: string): string {
	return WORK_ITEM_PRIORITIES.find((x) => x.value === p)?.label ?? p;
}

export function priorityBadgeClass(p: string): string {
	switch (p) {
		case 'urgent': return 'bg-red-100 text-red-700';
		case 'high': return 'bg-orange-100 text-orange-700';
		case 'medium': return 'bg-slate-100 text-slate-600';
		default: return 'bg-slate-50 text-slate-500';
	}
}

// --- Faz 4: Workflow Designer ---

export interface WorkflowTemplate {
	id: string;
	name: string;
	description: string | null;
	status: string;
	published_version: number | null;
	published_node_count: number | null;
	draft_node_count: number | null;
}

export interface WorkflowNode {
	id: string;
	name: string;
	description: string | null;
	node_type: string;
	sort_order: number;
	approval_rule?: string | null; // JSON string: {required, approver_role_id}
	default_assignee_type?: string | null; // user | team | null
	default_assignee_id?: string | null;
}

export interface WorkflowDependency {
	id: string;
	predecessor_node_id: string;
	successor_node_id: string;
	dependency_type: string;
}

export interface WorkflowDraft {
	version_id: string;
	version_number: number;
	nodes: WorkflowNode[];
	dependencies: WorkflowDependency[];
}

export interface WorkflowVersion {
	id: string;
	version_number: number;
	status: string;
	published_at: string | null;
	node_count: number;
}

// --- Faz 5: Workflow Runtime ---

export interface ProcessInstance {
	id: string;
	node_id: string;
	name: string;
	sort_order: number;
	status: string;
	ready_at: string | null;
	started_at: string | null;
	finished_at: string | null;
	predecessor_ids: string | null; // JSON array string
	assignments: string | null; // JSON array string
	approval: string | null; // JSON object string
	attempt_count: number;
	current_attempt: number;
}

/** Süreç kartı için parse edilmiş atama */
export interface ParsedAssignment {
	id: string;
	type: string;
	assignee_id: string;
	name: string;
}

/** Süreç kartı için parse edilmiş onay */
export interface ParsedApproval {
	decision: string;
	note: string | null;
	decided_by_name: string | null;
	requested_by_name: string | null;
}

export function parseAssignments(p: ProcessInstance): ParsedAssignment[] {
	if (!p.assignments) return [];
	try {
		const arr = JSON.parse(p.assignments);
		return Array.isArray(arr) ? arr : [];
	} catch {
		return [];
	}
}

export function parseApproval(p: ProcessInstance): ParsedApproval | null {
	if (!p.approval) return null;
	try {
		const o = JSON.parse(p.approval);
		return o && typeof o === 'object' ? o : null;
	} catch {
		return null;
	}
}

export interface ReworkCycle {
	id: string;
	restart_from_name: string;
	triggered_by_name: string;
	reason: string;
	created_by_name: string;
	created_at: string;
}

export interface WorkflowInstance {
	id: string;
	status: string;
	started_at: string | null;
	completed_at: string | null;
	template_name: string;
	version_number: number;
	processes: ProcessInstance[];
	total: number;
	approved: number;
	rework_cycles: ReworkCycle[];
}

export const PROCESS_STATUS = {
	waiting: { label: 'Bekliyor', badge: 'bg-slate-100 text-slate-500' },
	ready: { label: 'Hazır', badge: 'bg-blue-50 text-blue-700' },
	in_progress: { label: 'Devam Ediyor', badge: 'bg-amber-50 text-amber-700' },
	submitted: { label: 'Onay Bekliyor', badge: 'bg-violet-50 text-violet-700' },
	approved: { label: 'Tamamlandı', badge: 'bg-emerald-50 text-emerald-700' },
	failed: { label: 'Başarısız', badge: 'bg-red-50 text-red-700' },
	cancelled: { label: 'İptal', badge: 'bg-slate-100 text-slate-400' },
	skipped: { label: 'Atlandı', badge: 'bg-slate-100 text-slate-400' }
} as const;

export function processStatus(s: string): { label: string; badge: string } {
	return PROCESS_STATUS[s as keyof typeof PROCESS_STATUS] ?? { label: s, badge: 'bg-slate-100 text-slate-500' };
}

// --- Faz 8: Yorum / Dosya / Audit ---

export interface Comment {
	id: string;
	entity_type: string;
	entity_id: string;
	body: string;
	created_by: string;
	created_at: string;
	edited_at: string | null;
	author_name: string;
}

export interface Attachment {
	id: string;
	entity_type: string;
	entity_id: string;
	file_name: string;
	mime_type: string;
	size: number;
	created_at: string;
	uploader_name: string;
}

export function fileIcon(mime: string): string {
	if (mime.startsWith('image/')) return 'package';
	if (mime.startsWith('video/')) return 'play';
	if (mime === 'application/pdf') return 'file';
	if (mime.includes('zip') || mime.includes('rar')) return 'package';
	if (mime.startsWith('audio/')) return 'play';
	return 'paperclip';
}

export function fileSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export interface AuditEntry {
	id: string;
	action: string;
	entity_type: string | null;
	entity_id: string | null;
	metadata_json: string | null;
	created_at: string;
	actor_name: string | null;
}

export function auditLabel(action: string): string {
	switch (action) {
		case 'process.started': return 'süreci başlattı';
		case 'process.approved': return 'süreci onayladı';
		case 'process.rejected': return 'süreci reddetti';
		case 'rework.started': return 'rework başlattı';
		case 'workflow.published': return 'akış yayınladı';
		case 'workflow.assigned': return 'iş kalemine akış atadı';
		case 'member.added': return 'üye ekledi';
		case 'member.removed': return 'üye çıkardı';
		default: return action;
	}
}

export function auditIcon(action: string): string {
	switch (action) {
		case 'process.started': return 'play';
		case 'process.approved': return 'check-circle';
		case 'process.rejected': return 'x-circle';
		case 'rework.started': return 'rotate';
		case 'workflow.published': return 'upload';
		case 'workflow.assigned': return 'git-branch';
		case 'member.added': return 'user';
		case 'member.removed': return 'log-out';
		default: return 'activity';
	}
}

// --- Faz 9: Bildirimler ---

export interface Notification {
	id: string;
	type: string;
	title: string;
	message: string;
	entity_type: string | null;
	entity_id: string | null;
	read_at: string | null;
	created_at: string;
}

export function notificationIcon(type: string): string {
	switch (type) {
		case 'process.assigned': return 'git-branch';
		case 'process.approval_required': return 'timer';
		case 'process.approved': return 'check-circle';
		case 'process.rejected': return 'x-circle';
		case 'rework.started': return 'rotate';
		default: return 'bell';
	}
}

export interface WorkspaceInvite {
	id: string;
	email: string;
	role_name: string;
	status: string;
	expires_at: string;
	created_at: string;
	invited_by_name: string;
}

export interface InviteInfo {
	workspace_name: string;
	email: string;
	inviter_name: string;
	role_name: string;
	expires_at: string;
	existing_account: boolean;
}

export interface OutboxEntry {
	id: string;
	to_email: string;
	subject: string;
	status: string;
	created_at: string;
	sent_at: string | null;
}

// --- Faz 10: Dashboard ---

export interface NameCount {
	name: string;
	cnt: number;
}

export interface NameAvg {
	name: string;
	avg_seconds: number;
}

export interface Dashboard {
	active_items: number;
	completed_items: number;
	overdue_items: number;
	ready_processes: number;
	in_progress_processes: number;
	pending_approval_processes: number;
	failed_processes: number;
	rework_count: number;
	member_count: number;
	avg_cycle_time_hours: number | null;
	first_pass_success_rate: number | null;
	rework_rate: number | null;
	top_failed_processes: NameCount[];
	process_durations: NameAvg[];
}

export function fmtDuration(seconds: number): string {
	if (seconds < 60) return `${Math.round(seconds)} sn`;
	if (seconds < 3600) return `${Math.round(seconds / 60)} dk`;
	if (seconds < 86400) return `${(seconds / 3600).toFixed(1)} sa`;
	return `${(seconds / 86400).toFixed(1)} gün`;
}

export function parseJsonArray(v: string | null | undefined): string[] {
	if (!v) return [];
	try {
		const arr = JSON.parse(v);
		return Array.isArray(arr) ? arr : [];
	} catch {
		return [];
	}
}
