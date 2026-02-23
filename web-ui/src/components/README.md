# 组件说明

## PasswordStrengthIndicator - 密码强度指示器

### 功能概述
实时显示密码强度和要求满足情况的可视化组件。

### 使用方法

```tsx
import PasswordStrengthIndicator from './components/PasswordStrengthIndicator';

function MyForm() {
  const [password, setPassword] = useState('');
  
  return (
    <div>
      <input 
        type="password" 
        value={password}
        onChange={(e) => setPassword(e.target.value)}
      />
      <PasswordStrengthIndicator password={password} />
    </div>
  );
}
```

### 属性说明

| 属性 | 类型 | 必需 | 默认值 | 说明 |
|------|------|------|--------|------|
| password | string | 是 | - | 要检查的密码字符串 |
| showRequirements | boolean | 否 | true | 是否显示要求检查列表 |

### 显示效果

#### 1. 空密码
不显示任何内容

#### 2. 弱密码（红色）
```
密码强度: 弱
[■□□□]
✗ 至少 8 个字符
✗ 包含字母
✓ 包含数字
```

#### 3. 中等密码（黄色）
```
密码强度: 中等
[■■□□]
✓ 至少 8 个字符
✓ 包含字母
✓ 包含数字
💡 提示：使用 12 位以上、包含大小写字母和特殊字符的密码更安全
```

#### 4. 强密码（绿色）
```
密码强度: 强
[■■■□]
✓ 至少 8 个字符
✓ 包含字母
✓ 包含数字
💡 提示：使用 12 位以上、包含大小写字母和特殊字符的密码更安全
```

#### 5. 非常强密码（深绿色）
```
密码强度: 非常强
[■■■■]
✓ 至少 8 个字符
✓ 包含字母
✓ 包含数字
💡 提示：使用 12 位以上、包含大小写字母和特殊字符的密码更安全
```

### 强度评分规则

#### 基础要求（必须满足）
- 长度 ≥ 8 字符
- 包含至少 1 个字母
- 包含至少 1 个数字

#### 加分项
- 长度 ≥ 12 字符
- 长度 ≥ 16 字符
- 大小写字母混合
- 包含特殊字符
- 字符多样性高

### 样式定制

组件使用 Tailwind CSS，支持深色模式：

```tsx
// 强度条颜色
bg-red-500    // 弱
bg-yellow-500 // 中等
bg-green-500  // 强
bg-green-600  // 非常强

// 深色模式自动适配
dark:bg-gray-700
dark:text-gray-400
```

### 示例代码

#### 基本使用
```tsx
<PasswordStrengthIndicator password={password} />
```

#### 隐藏要求列表
```tsx
<PasswordStrengthIndicator 
  password={password} 
  showRequirements={false} 
/>
```

#### 完整表单示例
```tsx
function RegisterForm() {
  const [formData, setFormData] = useState({
    username: '',
    password: '',
    confirmPassword: ''
  });

  return (
    <form>
      <div>
        <label>用户名</label>
        <input 
          type="text"
          value={formData.username}
          onChange={(e) => setFormData({...formData, username: e.target.value})}
        />
      </div>
      
      <div>
        <label>密码</label>
        <input 
          type="password"
          value={formData.password}
          onChange={(e) => setFormData({...formData, password: e.target.value})}
        />
        <PasswordStrengthIndicator password={formData.password} />
      </div>
      
      <div>
        <label>确认密码</label>
        <input 
          type="password"
          value={formData.confirmPassword}
          onChange={(e) => setFormData({...formData, confirmPassword: e.target.value})}
        />
      </div>
      
      <button type="submit">注册</button>
    </form>
  );
}
```

### 性能考虑

- 使用 `useMemo` 缓存计算结果
- 仅在密码变化时重新计算
- 轻量级实现，无性能影响

### 可访问性

- 使用语义化 HTML
- 颜色搭配符合 WCAG 标准
- 支持屏幕阅读器

### 浏览器支持

- 现代浏览器（Chrome, Firefox, Safari, Edge）
- 移动浏览器
- 需要支持 ES6+ 和 CSS Grid


---

## BitrateHelper - 码率建议帮助组件

### 功能概述
显示不同分辨率和帧率组合下的推荐码率，以圆形问号图标形式呈现，鼠标悬停显示详细信息。

### 使用方法

```tsx
import BitrateHelper from './BitrateHelper';

function SettingsForm() {
  return (
    <div>
      <label className="flex items-center">
        码率 (Kbps)
        <BitrateHelper />
      </label>
      <input type="number" />
    </div>
  );
}
```

### 属性说明

| 属性 | 类型 | 必需 | 默认值 | 说明 |
|------|------|------|--------|------|
| className | string | 否 | '' | 自定义CSS类名 |

### 显示效果

#### 悬停前
```
码率 (Kbps) ⓘ
```

#### 悬停后
```
┌─────────────────────────────┐
│ 推荐码率参考                 │
│                             │
│ 4K (3840×2160)              │
│ • 30fps: 13000-18000 Kbps   │
│ • 60fps: 20000-25000 Kbps   │
│                             │
│ 1080p (1920×1080)           │
│ • 30fps: 4000-6000 Kbps     │
│ • 60fps: 6000-9000 Kbps     │
│                             │
│ 720p (1280×720)             │
│ • 30fps: 2500-4000 Kbps     │
│ • 60fps: 4000-6000 Kbps     │
│                             │
│ 480p (854×480)              │
│ • 30fps: 1000-2000 Kbps     │
│ • 60fps: 1500-3000 Kbps     │
│                             │
│ 说明：                       │
│ • 码率越高，画质越好，文件越大│
│ • 网络推流建议使用较低码率   │
│ • 本地录制可使用较高码率     │
└─────────────────────────────┘
```

### 推荐码率表

| 分辨率 | 30fps | 60fps |
|--------|-------|-------|
| 4K (3840×2160) | 13000-18000 | 20000-25000 |
| 1080p (1920×1080) | 4000-6000 | 6000-9000 |
| 720p (1280×720) | 2500-4000 | 4000-6000 |
| 480p (854×480) | 1000-2000 | 1500-3000 |

### 交互行为

- 鼠标移入图标 → 显示提示框
- 鼠标移出图标 → 隐藏提示框
- Tab键聚焦 → 显示提示框
- 失去焦点 → 隐藏提示框

### 集成位置

1. **管理员页面** (`AdminDashboard.tsx`)
   - 系统设置 → 全局录制限制 → 默认最大码率

2. **用户设置** (`UserSettingsModal.tsx`)
   - 录制设置 → 码率

### 样式定制

组件使用 Tailwind CSS，支持深色模式：

```tsx
// 自定义间距
<BitrateHelper className="ml-4" />

// 图标颜色会自动适配主题
```

### 示例代码

#### 管理员设置
```tsx
<div>
  <label className="block text-sm font-medium mb-1 flex items-center">
    默认最大码率 (Kbps)
    <BitrateHelper />
  </label>
  <input 
    type="number" 
    value={bitrate}
    onChange={e => setBitrate(e.target.value)}
    className="w-full p-2 border rounded"
  />
</div>
```

#### 用户设置
```tsx
<div className="grid grid-cols-2 gap-4">
  <div>
    <label className="flex items-center">
      帧率 (FPS)
    </label>
    <input type="number" />
  </div>
  <div>
    <label className="flex items-center">
      码率 (Kbps)
      <BitrateHelper />
    </label>
    <input type="number" />
  </div>
</div>
```

### 码率选择建议

#### 本地录制
- 使用推荐范围的上限
- 优先保证画质
- 适合后期编辑

#### 网络推流
- 使用推荐范围的下限
- 考虑带宽限制
- 避免卡顿

#### 移动设备
- 使用较低码率
- 节省流量
- 保证流畅

### 性能考虑

- 轻量级实现
- 按需渲染
- 无性能影响
- 支持键盘导航

### 可访问性

- 支持键盘操作
- 颜色对比符合标准
- 语义化HTML

### 浏览器支持

- 现代浏览器（Chrome, Firefox, Safari, Edge）
- 移动浏览器
- 需要支持 CSS Grid 和 Flexbox
