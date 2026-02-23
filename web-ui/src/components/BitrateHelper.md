# BitrateHelper 组件说明

## 概述
码率建议帮助组件，以圆形问号图标形式显示，鼠标悬停时显示不同分辨率和帧率组合下的推荐码率。

## 功能特性
- 🎯 圆形问号图标（HelpCircle）
- 🖱️ 鼠标悬停显示/移开隐藏
- ⌨️ 键盘焦点支持（无障碍）
- 🎨 黑色半透明背景（95%不透明度）
- 📍 智能定位：按钮右侧显示
- 📏 动态垂直位置调整，确保内容完整显示
- 🌫️ 背景模糊效果（backdrop-blur）
- 📱 响应式设计

## 使用方法

### 基本使用
```tsx
import BitrateHelper from './BitrateHelper';

<label className="flex items-center">
  码率 (Kbps)
  <BitrateHelper />
</label>
```

### 自定义样式
```tsx
<BitrateHelper className="ml-4" />
```

## 显示内容

### 推荐码率表

#### 4K (3840×2160)
- 30fps: 13000-18000 Kbps
- 60fps: 20000-25000 Kbps

#### 1080p (1920×1080)
- 30fps: 4000-6000 Kbps
- 60fps: 6000-9000 Kbps

#### 720p (1280×720)
- 30fps: 2500-4000 Kbps
- 60fps: 4000-6000 Kbps

#### 480p (854×480)
- 30fps: 1000-2000 Kbps
- 60fps: 1500-3000 Kbps

### 使用说明
- 码率越高，画质越好，文件越大
- 网络推流建议使用较低码率
- 本地录制可使用较高码率

## 集成位置

### 管理员页面
**文件**: `web-ui/src/components/AdminDashboard.tsx`

**位置**: 系统设置 → 全局录制限制 → 默认最大码率

```tsx
<label className="block text-sm font-medium mb-1 flex items-center">
    默认最大码率 (Kbps)
    <BitrateHelper />
</label>
```

### 用户设置页面
**文件**: `web-ui/src/components/UserSettingsModal.tsx`

**位置**: 录制设置 → 码率

```tsx
<label className="block text-sm font-medium mb-1 flex items-center">
    码率 (Kbps)
    <BitrateHelper />
</label>
```

## 交互行为

### 鼠标交互
1. 鼠标移入问号图标 → 显示提示框
2. 鼠标移出问号图标 → 隐藏提示框
3. 鼠标悬停在提示框上 → 保持显示

### 键盘交互
1. Tab 键聚焦到问号图标 → 显示提示框
2. 失去焦点 → 隐藏提示框

### 样式说明

### 图标样式
- 默认颜色: `text-gray-400`
- 悬停颜色: `text-blue-500`
- 尺寸: 18px
- 过渡效果: 颜色平滑过渡
- 焦点样式: 蓝色光圈

### 提示框样式
- 宽度: 320px (w-80)
- 背景: 黑色半透明 (bg-gray-900 bg-opacity-95)
- 文字颜色: 白色
- 边框: 无边框
- 圆角: rounded-lg
- 阴影: 超大阴影 (shadow-2xl)
- 背景模糊: backdrop-blur-sm
- 定位: 按钮右侧，动态垂直位置
- 层级: z-[9999] (确保在所有模态框之上)

### 定位逻辑
```typescript
// 智能垂直定位
1. 默认与按钮顶部对齐
2. 检查下方空间是否足够
3. 如果不足且上方空间更大，向上偏移
4. 确保提示框完整显示在视口内
```

### 内容样式
- 标题: 14px 半粗体，浅灰色
- 分辨率标签: 蓝色高亮 (text-blue-400)
- 码率数值: 12px 浅灰色 (text-gray-300)
- 分隔线: 深灰色边框 (border-gray-700)
- 说明文字: 灰色 (text-gray-400)

## 技术实现

### 状态管理
```tsx
const [show, setShow] = useState(false);
const [position, setPosition] = useState({ top: 0 });
const buttonRef = useRef<HTMLButtonElement>(null);
const tooltipRef = useRef<HTMLDivElement>(null);
```

### 智能定位算法
```tsx
useEffect(() => {
  if (show && buttonRef.current && tooltipRef.current) {
    const buttonRect = buttonRef.current.getBoundingClientRect();
    const tooltipRect = tooltipRef.current.getBoundingClientRect();
    const viewportHeight = window.innerHeight;
    
    let top = 0;
    const spaceBelow = viewportHeight - buttonRect.top;
    const spaceAbove = buttonRect.top;
    
    // 智能调整垂直位置
    if (tooltipRect.height > spaceBelow && spaceAbove > spaceBelow) {
      top = -(tooltipRect.height - buttonRect.height);
    } else if (tooltipRect.height > spaceBelow) {
      top = -(tooltipRect.height - spaceBelow + 20);
    }
    
    setPosition({ top });
  }
}, [show]);
```

### 事件处理
```tsx
onMouseEnter={() => setShow(true)}
onMouseLeave={() => setShow(false)}
onFocus={() => setShow(true)}
onBlur={() => setShow(false)}
```

### 条件渲染
```tsx
{show && (
  <div 
    ref={tooltipRef}
    className="absolute left-full ml-2 z-50 w-80"
    style={{ top: `${position.top}px` }}
  >
    {/* 提示内容 */}
  </div>
)}
```

## 码率推荐依据

### 计算公式
```
码率 (Kbps) ≈ 分辨率 × 帧率 × 压缩比
```

### 压缩比说明
- H.264/AVC: 0.07-0.10 bits/pixel
- H.265/HEVC: 0.04-0.06 bits/pixel

### 推荐范围
- 下限: 保证基本画质
- 上限: 平衡画质和文件大小

## 使用场景

### 管理员
- 设置全局默认码率上限
- 为所有用户提供参考标准
- 控制服务器存储和带宽

### 普通用户
- 根据需求选择合适码率
- 平衡画质和文件大小
- 优化推流质量

## 最佳实践

### 本地录制
- 使用推荐范围的上限
- 优先保证画质
- 存储空间充足时可适当提高

### 网络推流
- 使用推荐范围的下限
- 考虑网络带宽限制
- 避免卡顿和延迟

### 移动设备
- 使用较低分辨率和码率
- 节省流量和电量
- 保证流畅播放

## 可访问性

### 键盘导航
- 支持 Tab 键聚焦
- 支持焦点显示提示

### 屏幕阅读器
- 按钮有明确的语义
- 提示内容可被读取

### 颜色对比
- 符合 WCAG 标准
- 深色模式适配

## 浏览器兼容性
- Chrome/Edge 90+
- Firefox 88+
- Safari 14+
- 移动浏览器

## 性能优化
- 轻量级实现
- 无外部依赖（除 lucide-react）
- 按需渲染
- 无性能影响

## 未来改进

### 功能增强
- [ ] 根据当前选择的分辨率和帧率高亮推荐码率
- [ ] 添加自定义码率计算器
- [ ] 支持更多编码器的推荐值
- [ ] 添加实时预览文件大小
- [x] 智能定位，确保内容完整显示
- [x] 黑色半透明背景优化视觉效果

### 交互优化
- [ ] 添加点击固定显示功能
- [ ] 支持移动端触摸交互
- [ ] 添加淡入淡出动画效果
- [x] 按钮右侧显示，避免遮挡内容

### 内容扩展
- [ ] 添加不同编码器的对比
- [ ] 提供更详细的技术说明
- [ ] 支持多语言

## 相关文档
- [组件使用文档](README.md)
- [前端改进说明](../../FRONTEND_PASSWORD_IMPROVEMENTS.md)
- [中文化检查清单](../../FRONTEND_I18N_CHECKLIST.md)
