import { useMemo } from 'react';

interface PasswordStrengthIndicatorProps {
  password: string;
  showRequirements?: boolean;
}

interface PasswordRequirement {
  label: string;
  met: boolean;
}

export default function PasswordStrengthIndicator({ 
  password, 
  showRequirements = true 
}: PasswordStrengthIndicatorProps) {
  
  const requirements: PasswordRequirement[] = useMemo(() => {
    return [
      {
        label: '至少 8 个字符',
        met: password.length >= 8
      },
      {
        label: '包含字母',
        met: /[a-zA-Z]/.test(password)
      },
      {
        label: '包含数字',
        met: /[0-9]/.test(password)
      }
    ];
  }, [password]);

  const strength = useMemo(() => {
    if (password.length === 0) return { level: 0, label: '', color: '' };
    
    let score = 0;
    const metCount = requirements.filter(r => r.met).length;
    
    // 基础分数：满足所有基本要求
    if (metCount === 3) score += 40;
    else score += metCount * 10;
    
    // 长度加分
    if (password.length >= 8) score += 10;
    if (password.length >= 12) score += 15;
    if (password.length >= 16) score += 15;
    
    // 复杂度加分
    if (/[a-z]/.test(password) && /[A-Z]/.test(password)) score += 10; // 大小写混合
    if (/[!@#$%^&*()_+\-=\[\]{};':"\\|,.<>\/?]/.test(password)) score += 10; // 特殊字符
    
    // 多样性加分
    const uniqueChars = new Set(password).size;
    if (uniqueChars >= password.length * 0.7) score += 10;
    
    if (score < 40) return { level: 1, label: '弱', color: 'bg-red-500' };
    if (score < 60) return { level: 2, label: '中等', color: 'bg-yellow-500' };
    if (score < 80) return { level: 3, label: '强', color: 'bg-green-500' };
    return { level: 4, label: '非常强', color: 'bg-green-600' };
  }, [password, requirements]);

  const allRequirementsMet = requirements.every(r => r.met);

  if (password.length === 0) return null;

  return (
    <div className="mt-2 space-y-2">
      {/* 强度条 */}
      <div className="space-y-1">
        <div className="flex items-center justify-between text-xs">
          <span className="text-gray-600 dark:text-gray-400">密码强度</span>
          <span className={`font-medium ${
            strength.level === 1 ? 'text-red-600' :
            strength.level === 2 ? 'text-yellow-600' :
            strength.level === 3 ? 'text-green-600' :
            'text-green-700'
          }`}>
            {strength.label}
          </span>
        </div>
        <div className="flex gap-1 h-1.5">
          {[1, 2, 3, 4].map((level) => (
            <div
              key={level}
              className={`flex-1 rounded-full transition-colors ${
                level <= strength.level
                  ? strength.color
                  : 'bg-gray-200 dark:bg-gray-700'
              }`}
            />
          ))}
        </div>
      </div>

      {/* 要求列表 */}
      {showRequirements && (
        <div className="space-y-1">
          {requirements.map((req, index) => (
            <div
              key={index}
              className={`flex items-center gap-2 text-xs transition-colors ${
                req.met
                  ? 'text-green-600 dark:text-green-400'
                  : 'text-gray-500 dark:text-gray-400'
              }`}
            >
              <span className="flex-shrink-0">
                {req.met ? (
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                  </svg>
                ) : (
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
                  </svg>
                )}
              </span>
              <span>{req.label}</span>
            </div>
          ))}
          {allRequirementsMet && (
            <div className="text-xs text-gray-500 dark:text-gray-400 mt-2 pt-2 border-t border-gray-200 dark:border-gray-700">
              💡 提示：使用 12 位以上、包含大小写字母和特殊字符的密码更安全
            </div>
          )}
        </div>
      )}
    </div>
  );
}
