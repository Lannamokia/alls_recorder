import { HelpCircle } from 'lucide-react';
import { useState, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';

interface BitrateHelperProps {
  className?: string;
}

export default function BitrateHelper({ className = '' }: BitrateHelperProps) {
  const [show, setShow] = useState(false);
  const [position, setPosition] = useState({ top: 0, left: 0 });
  const buttonRef = useRef<HTMLButtonElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const hideTimeoutRef = useRef<number | null>(null);

  const handleMouseEnter = () => {
    if (hideTimeoutRef.current) {
      clearTimeout(hideTimeoutRef.current);
      hideTimeoutRef.current = null;
    }
    setShow(true);
  };

  const handleMouseLeave = () => {
    hideTimeoutRef.current = window.setTimeout(() => {
      setShow(false);
    }, 100);
  };

  useEffect(() => {
    if (show && buttonRef.current) {
      // 使用 requestAnimationFrame 确保 DOM 已更新
      requestAnimationFrame(() => {
        if (!buttonRef.current) return;
        
        const buttonRect = buttonRef.current.getBoundingClientRect();
        const viewportHeight = window.innerHeight;
        const viewportWidth = window.innerWidth;
        
        // 提示框宽度
        const tooltipWidth = 320;
        const tooltipHeight = tooltipRef.current?.offsetHeight || 450;
        
        // 计算水平位置（按钮右侧）
        let left = buttonRect.right + 8;
        
        // 如果右侧空间不足，显示在左侧
        if (left + tooltipWidth > viewportWidth) {
          left = buttonRect.left - tooltipWidth - 8;
        }
        
        // 计算垂直位置
        let top = buttonRect.top;
        
        // 检查是否会超出底部
        const spaceBelow = viewportHeight - buttonRect.top;
        const spaceAbove = buttonRect.top;
        
        if (tooltipHeight > spaceBelow && spaceAbove > spaceBelow) {
          // 如果下方空间不足且上方空间更大，向上对齐
          top = Math.max(10, buttonRect.bottom - tooltipHeight);
        } else if (tooltipHeight > spaceBelow) {
          // 如果下方空间不足，调整到能完整显示的位置
          top = Math.max(10, viewportHeight - tooltipHeight - 20);
        }
        
        setPosition({ top, left });
      });
    }
  }, [show]);

  useEffect(() => {
    return () => {
      if (hideTimeoutRef.current) {
        clearTimeout(hideTimeoutRef.current);
      }
    };
  }, []);

  const tooltip = show ? createPortal(
    <div
      ref={tooltipRef}
      className="fixed z-[9999] w-80"
      style={{ top: `${position.top}px`, left: `${position.left}px` }}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      <div className="bg-gray-900 bg-opacity-95 text-white rounded-lg shadow-2xl p-4 backdrop-blur-sm border border-gray-700">
        <h4 className="font-semibold mb-3 text-sm text-gray-100">推荐码率参考</h4>
        
        <div className="space-y-3 text-xs">
          {/* 4K */}
          <div className="border-b border-gray-700 pb-2">
            <div className="font-medium text-blue-400 mb-1">4K (3840×2160)</div>
            <div className="space-y-1 text-gray-300">
              <div>• 30fps: 13000-18000 Kbps</div>
              <div>• 60fps: 20000-25000 Kbps</div>
            </div>
          </div>

          {/* 1080p */}
          <div className="border-b border-gray-700 pb-2">
            <div className="font-medium text-blue-400 mb-1">1080p (1920×1080)</div>
            <div className="space-y-1 text-gray-300">
              <div>• 30fps: 4000-6000 Kbps</div>
              <div>• 60fps: 6000-9000 Kbps</div>
            </div>
          </div>

          {/* 720p */}
          <div className="border-b border-gray-700 pb-2">
            <div className="font-medium text-blue-400 mb-1">720p (1280×720)</div>
            <div className="space-y-1 text-gray-300">
              <div>• 30fps: 2500-4000 Kbps</div>
              <div>• 60fps: 4000-6000 Kbps</div>
            </div>
          </div>

          {/* 480p */}
          <div>
            <div className="font-medium text-blue-400 mb-1">480p (854×480)</div>
            <div className="space-y-1 text-gray-300">
              <div>• 30fps: 1000-2000 Kbps</div>
              <div>• 60fps: 1500-3000 Kbps</div>
            </div>
          </div>
        </div>

        <div className="mt-3 pt-3 border-t border-gray-700 text-xs text-gray-400">
          <div className="font-medium mb-1 text-gray-300">说明：</div>
          <ul className="space-y-1 list-disc list-inside">
            <li>码率越高，画质越好，文件越大</li>
            <li>网络推流建议使用较低码率</li>
            <li>本地录制可使用较高码率</li>
          </ul>
        </div>
      </div>
    </div>,
    document.body
  ) : null;

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className={`text-gray-400 hover:text-blue-500 transition-colors ml-2 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-opacity-50 rounded-full ${className}`}
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        onFocus={handleMouseEnter}
        onBlur={handleMouseLeave}
        aria-label="查看推荐码率"
      >
        <HelpCircle size={18} />
      </button>
      {tooltip}
    </>
  );
}
